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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub download_url: String,
    pub sha256: Option<String>,
    pub changelog: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalVersionInfo {
    pub version: String,
    pub channel: String,
    pub updated_at: String,
}

pub struct Updater {
    client: Client,
    client_dir: PathBuf,
}

impl Updater {
    pub fn new(client_dir: PathBuf) -> Self {
        Self {
            client: Client::builder()
                .user_agent("Quaternion-Launcher/0.1.0")
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

    pub async fn check_and_update(&self, endpoint: &str, channel: &str) -> Result<bool> {
        fs::create_dir_all(&self.client_dir)?;

        let manifest = match self.fetch_manifest(endpoint, channel).await {
            Ok(m) => m,
            Err(e) => {
                println!(
                    "{} Warning: Could not check for updates ({}). Proceeding with local client.",
                    "[!]".yellow(),
                    e
                );
                return Ok(false);
            }
        };

        let local_info = self.get_local_version();
        let needs_update = match &local_info {
            Some(local) => local.version != manifest.version,
            None => true,
        };

        if !needs_update {
            println!(
                "{} Client is up to date (v{}).",
                "[✓]".green(),
                manifest.version
            );
            return Ok(false);
        }

        println!(
            "{} New version found: {} -> {}",
            "[+]".green().bold(),
            local_info.map(|l| l.version).unwrap_or_else(|| "none".to_string()),
            manifest.version.bold()
        );

        if let Some(ref changelog) = manifest.changelog {
            println!("\n{} Changelog:\n{}\n", "[*]".cyan(), changelog);
        }

        let temp_dir = tempfile::tempdir()?;
        let archive_path = temp_dir.path().join("quaternion_update.zip");

        self.download_file(&manifest.download_url, &archive_path, manifest.sha256.as_deref())
            .await?;

        println!("{} Installing update...", "[*]".cyan());
        self.extract_zip(&archive_path, &self.client_dir)?;

        let new_local_info = LocalVersionInfo {
            version: manifest.version.clone(),
            channel: channel.to_string(),
            updated_at: chrono_like_now(),
        };
        let info_json = serde_json::to_string_pretty(&new_local_info)?;
        fs::write(self.client_dir.join("version.json"), info_json)?;

        println!(
            "{} Successfully updated Quaternion to v{}!\n",
            "[✓]".green().bold(),
            manifest.version
        );

        Ok(true)
    }

    async fn download_file(&self, url: &str, dest: &Path, expected_sha256: Option<&str>) -> Result<()> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("Failed to initiate download")?;

        let total_size = response
            .content_length()
            .unwrap_or(0);

        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("#>-"),
        );

        let mut file = File::create(dest).context("Failed to create temporary archive file")?;
        let mut stream = response.bytes_stream();
        let mut hasher = Sha256::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.context("Error downloading chunk")?;
            file.write_all(&chunk)?;
            hasher.update(&chunk);
            pb.inc(chunk.len() as u64);
        }

        pb.finish_with_message("Download complete");

        if let Some(expected_hash) = expected_sha256 {
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

    fn extract_zip(&self, zip_path: &Path, target_dir: &Path) -> Result<()> {
        let file = File::open(zip_path).context("Failed to open downloaded archive")?;
        let mut archive = zip::ZipArchive::new(file).context("Failed to read zip archive")?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let outpath = match file.enclosed_name() {
                Some(path) => target_dir.join(path),
                None => continue,
            };

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
    // Simple ISO timestamp representation without extra dependencies
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("{}", d.as_secs()))
        .unwrap_or_default()
}
