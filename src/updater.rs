use anyhow::{anyhow, Context, Result};
use colored::*;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub trait UpdateProgressObserver: Send + Sync {
    fn on_status(&self, message: &str);
    fn on_progress(&self, fraction: f32, message: &str);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub download_url: String,
    pub sha256: Option<String>,
    pub launcher_version: Option<String>,
    pub launcher_url: Option<String>,
    pub launcher_sha256: Option<String>,
    pub changelog: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalVersionInfo {
    pub version: String,
    pub channel: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct PendingUpdate {
    pub manifest: UpdateManifest,
    pub needs_launcher_update: bool,
    pub needs_client_update: bool,
    pub current_client_version: Option<String>,
}

pub struct Updater {
    client: Client,
    client_dir: PathBuf,
}

impl Updater {
    pub fn new(client_dir: PathBuf) -> Self {
        Self {
            client: Client::builder()
                .user_agent(concat!("Quaternion-Launcher/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
            client_dir,
        }
    }

    pub fn get_local_version(&self) -> Option<LocalVersionInfo> {
        let version_path = self.client_dir.join("version.json");
        if !version_path.exists() {
            return None;
        }
        let content = fs::read_to_string(version_path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub async fn fetch_manifest(&self, endpoint: &str, channel: &str) -> Result<UpdateManifest> {
        let os_name = if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        };

        let separator = if endpoint.contains('?') { "&" } else { "?" };
        let url = format!("{}{}channel={}&os={}", endpoint, separator, channel, os_name);

        println!("{} Checking for updates from {}", "[*]".cyan(), url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to update server")?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Update server returned status code: {}",
                response.status()
            ));
        }

        let manifest = response
            .json::<UpdateManifest>()
            .await
            .context("Failed to parse update manifest JSON")?;

        Ok(manifest)
    }

    pub async fn check_for_updates(&self, endpoint: &str, channel: &str) -> Result<Option<PendingUpdate>> {
        fs::create_dir_all(&self.client_dir)?;

        let manifest = match self.fetch_manifest(endpoint, channel).await {
            Ok(m) => m,
            Err(e) => {
                println!(
                    "{} Warning: Could not check for updates ({}). Proceeding with local client.",
                    "[!]".yellow(),
                    e
                );
                return Ok(None);
            }
        };

        let current_launcher_version = env!("CARGO_PKG_VERSION");
        let remote_launcher_version = manifest.launcher_version.as_deref().unwrap_or("");
        let is_dev_or_debug = cfg!(debug_assertions) || std::env::var("QUATERNION_DEV").is_ok();
        let needs_launcher_update = !is_dev_or_debug
            && !remote_launcher_version.is_empty()
            && remote_launcher_version != current_launcher_version
            && manifest.launcher_url.as_ref().map_or(false, |u| !u.is_empty());

        let local_info = self.get_local_version();
        let needs_client_update = match &local_info {
            Some(local) => local.version != manifest.version,
            None => true,
        };

        if !needs_launcher_update && !needs_client_update {
            println!(
                "{} Client is up to date (v{}).",
                "[✓]".green(),
                manifest.version
            );
            return Ok(None);
        }

        println!(
            "{} New version found: {} -> {}",
            "[+]".green().bold(),
            local_info.as_ref().map(|l| l.version.clone()).unwrap_or_else(|| "none".to_string()),
            manifest.version.bold()
        );

        if let Some(ref changelog) = manifest.changelog {
            println!("\n{} Changelog:\n{}\n", "[*]".cyan(), changelog);
        }

        Ok(Some(PendingUpdate {
            manifest,
            needs_launcher_update,
            needs_client_update,
            current_client_version: local_info.map(|l| l.version),
        }))
    }

    pub async fn update_launcher_self(
        &self,
        manifest: &UpdateManifest,
        observer: Option<&dyn UpdateProgressObserver>,
    ) -> Result<bool> {
        if cfg!(debug_assertions) || std::env::var("QUATERNION_DEV").is_ok() {
            println!(
                "{} Skipping launcher self-update (debug/dev mode active).",
                "[*]".cyan()
            );
            return Ok(false);
        }

        let launcher_url = match &manifest.launcher_url {
            Some(u) if !u.is_empty() => u,
            _ => return Ok(false),
        };

        let current_version = env!("CARGO_PKG_VERSION");
        let remote_version = manifest.launcher_version.as_deref().unwrap_or("");

        if !remote_version.is_empty() && remote_version == current_version {
            return Ok(false);
        }

        let current_exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(_) => return Ok(false),
        };

        let old_exe = current_exe.with_extension("old");
        if old_exe.exists() {
            let _ = fs::remove_file(&old_exe);
        }

        if let Some(expected_sha) = manifest.launcher_sha256.as_deref() {
            if let Ok(current_bytes) = fs::read(&current_exe) {
                let current_hash = hex::encode(Sha256::digest(&current_bytes));
                if current_hash.eq_ignore_ascii_case(expected_sha) {
                    return Ok(false);
                }
            }
        }

        println!(
            "{} New launcher version found: {} -> {}",
            "[+]".green().bold(),
            current_version,
            if remote_version.is_empty() { "latest" } else { remote_version }.bold()
        );

        if let Some(obs) = observer {
            obs.on_status("Downloading launcher update...");
        }

        let temp_dir = tempfile::tempdir()?;
        let temp_launcher_path = temp_dir.path().join("quaternion_launcher_update");

        self.download_file(launcher_url, &temp_launcher_path, manifest.launcher_sha256.as_deref(), observer)
            .await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&temp_launcher_path, fs::Permissions::from_mode(0o755));
        }

        println!("{} Updating launcher executable...", "[*]".cyan());
        if let Some(obs) = observer {
            obs.on_status("Installing launcher executable...");
        }

        if let Err(e) = fs::rename(&current_exe, &old_exe) {
            println!(
                "{} Warning: Could not rename current launcher binary ({}). Skipping launcher update.",
                "[!]".yellow(),
                e
            );
            return Ok(false);
        }

        if let Err(e) = fs::copy(&temp_launcher_path, &current_exe) {
            let _ = fs::rename(&old_exe, &current_exe);
            return Err(anyhow!("Failed to install new launcher executable: {}", e));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&current_exe, fs::Permissions::from_mode(0o755));
            let _ = fs::remove_file(&old_exe);
        }

        println!(
            "{} Successfully updated Quaternion Launcher!\n",
            "[✓]".green().bold()
        );

        Ok(true)
    }

    pub async fn apply_update(
        &self,
        pending: &PendingUpdate,
        channel: &str,
        observer: Option<Arc<dyn UpdateProgressObserver>>,
    ) -> Result<bool> {
        let obs_ref = observer.as_deref();

        // 1. Self-update launcher if needed
        if pending.needs_launcher_update {
            if let Err(e) = self.update_launcher_self(&pending.manifest, obs_ref).await {
                println!(
                    "{} Warning: Launcher self-update failed: {}",
                    "[!]".yellow(),
                    e
                );
            }
        }

        // 2. Download and install client files if needed
        if pending.needs_client_update {
            if let Some(obs) = obs_ref {
                obs.on_status("Connecting to download server...");
            }

            let temp_dir = tempfile::tempdir()?;
            let archive_path = temp_dir.path().join("quaternion_update.zip");

            self.download_file(
                &pending.manifest.download_url,
                &archive_path,
                pending.manifest.sha256.as_deref(),
                obs_ref,
            )
            .await?;

            println!("{} Installing update...", "[*]".cyan());
            if let Some(obs) = obs_ref {
                obs.on_status("Extracting update files...");
            }

            self.extract_zip(&archive_path, &self.client_dir, obs_ref)?;

            let new_local_info = LocalVersionInfo {
                version: pending.manifest.version.clone(),
                channel: channel.to_string(),
                updated_at: chrono_like_now(),
            };
            let info_json = serde_json::to_string_pretty(&new_local_info)?;
            fs::write(self.client_dir.join("version.json"), info_json)?;

            println!(
                "{} Successfully updated Quaternion to v{}!\n",
                "[✓]".green().bold(),
                pending.manifest.version
            );

            if let Some(obs) = obs_ref {
                obs.on_progress(1.0, "Update complete! Starting game...");
            }
        }

        Ok(true)
    }

    #[allow(dead_code)]
    pub async fn check_and_update(&self, endpoint: &str, channel: &str) -> Result<bool> {
        let pending = match self.check_for_updates(endpoint, channel).await? {
            Some(p) => p,
            None => return Ok(false),
        };

        self.apply_update(&pending, channel, None).await
    }

    async fn download_file(
        &self,
        url: &str,
        dest: &Path,
        expected_sha256: Option<&str>,
        observer: Option<&dyn UpdateProgressObserver>,
    ) -> Result<()> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("Failed to initiate download")?;

        let total_size = response.content_length().unwrap_or(0);

        // Terminal progress bar when no GUI observer is attached
        let pb = if observer.is_none() {
            let p = ProgressBar::new(total_size);
            p.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                    .unwrap()
                    .progress_chars("#>-"),
            );
            Some(p)
        } else {
            None
        };

        let mut file = File::create(dest).context("Failed to create temporary archive file")?;
        let mut stream = response.bytes_stream();
        let mut hasher = Sha256::new();
        let mut downloaded: u64 = 0;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.context("Error downloading chunk")?;
            file.write_all(&chunk)?;
            hasher.update(&chunk);
            downloaded += chunk.len() as u64;

            if let Some(ref p) = pb {
                p.inc(chunk.len() as u64);
            }

            if let Some(obs) = observer {
                let fraction = if total_size > 0 {
                    downloaded as f32 / total_size as f32
                } else {
                    0.0
                };
                let status = if total_size > 0 {
                    format!(
                        "Downloading: {:.1} MB / {:.1} MB",
                        downloaded as f64 / 1_048_576.0,
                        total_size as f64 / 1_048_576.0
                    )
                } else {
                    format!("Downloading: {:.1} MB", downloaded as f64 / 1_048_576.0)
                };
                obs.on_progress(fraction, &status);
            }
        }

        if let Some(ref p) = pb {
            p.finish_with_message("Download complete");
        }

        if let Some(expected_hash) = expected_sha256 {
            if let Some(obs) = observer {
                obs.on_status("Verifying integrity checksum...");
            }
            let calculated_hash = hex::encode(hasher.finalize());
            if !calculated_hash.eq_ignore_ascii_case(expected_hash) {
                return Err(anyhow!(
                    "SHA256 checksum mismatch!\nExpected: {}\nGot:      {}",
                    expected_hash,
                    calculated_hash
                ));
            }
            println!("{} Checksum verified.", "[✓]".green());
        }

        Ok(())
    }

    fn extract_zip(
        &self,
        zip_path: &Path,
        target_dir: &Path,
        observer: Option<&dyn UpdateProgressObserver>,
    ) -> Result<()> {
        let file = File::open(zip_path).context("Failed to open downloaded archive")?;
        let mut archive = zip::ZipArchive::new(file).context("Failed to read zip archive")?;
        let total_files = archive.len();

        for i in 0..total_files {
            let mut file = archive.by_index(i)?;
            let outpath = match file.enclosed_name() {
                Some(path) => target_dir.join(path),
                None => continue,
            };

            if let Some(obs) = observer {
                if i % 15 == 0 || i + 1 == total_files {
                    let frac = (i + 1) as f32 / total_files.max(1) as f32;
                    obs.on_progress(frac, &format!("Extracting client files ({}/{})", i + 1, total_files));
                }
            }

            if file.is_dir() {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p)?;
                    }
                }
                let mut outfile = File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Some(mode) = file.unix_mode() {
                        fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
                    } else if outpath.extension().map_or(false, |ext| {
                        ext == "x86_64" || ext == "arm64" || ext == "dylib" || ext == "so" || ext == "sh"
                    }) || outpath.to_string_lossy().contains("MacOS/") {
                        fs::set_permissions(&outpath, fs::Permissions::from_mode(0o755))?;
                    }
                }
            }
        }

        Ok(())
    }
}

fn chrono_like_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("{}", d.as_secs()))
        .unwrap_or_default()
}
