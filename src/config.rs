use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "Quaternion";
const APPLICATION: &str = "Quaternion";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherConfig {
    /// Update API manifest endpoint
    pub update_url: String,
    /// Release channel (e.g., "stable", "beta", "nightly")
    pub channel: String,
    /// Whether to check for updates automatically before launch
    pub auto_update: bool,
    /// Custom path to client executable if not in default directory
    pub custom_client_path: Option<PathBuf>,
    /// Additional arguments to always pass to Godot
    pub extra_args: Vec<String>,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            update_url: "https://quaternion.vip/api/v1/launcher/version".to_string(),
            channel: "stable".to_string(),
            auto_update: true,
            custom_client_path: None,
            extra_args: Vec::new(),
        }
    }
}

pub struct PathManager;

impl PathManager {
    /// Get the directory where the launcher binary is located
    pub fn get_launcher_dir() -> Result<PathBuf> {
        let exe = std::env::current_exe().context("Failed to get current executable path")?;
        let dir = exe
            .parent()
            .context("Failed to get executable parent directory")?
            .to_path_buf();
        Ok(dir)
    }

    /// Get default client data directory (portable if `./client` exists, else user data dir)
    pub fn get_client_dir() -> Result<PathBuf> {
        let launcher_dir = Self::get_launcher_dir()?;
        let portable_client = launcher_dir.join("client");

        if portable_client.exists() {
            return Ok(portable_client);
        }

        // Check if executable exists directly in the launcher directory
        #[cfg(target_os = "windows")]
        let local_exe = launcher_dir.join("Polytoria.exe");
        #[cfg(not(target_os = "windows"))]
        let local_exe = launcher_dir.join("Polytoria.x86_64");

        if local_exe.exists() {
            return Ok(launcher_dir);
        }

        if let Some(proj_dirs) = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION) {
            let data_dir = proj_dirs.data_local_dir().join("client");
            fs::create_dir_all(&data_dir).context("Failed to create client directory")?;
            Ok(data_dir)
        } else {
            Ok(portable_client)
        }
    }

    /// Get path to config file
    pub fn get_config_path() -> Result<PathBuf> {
        let launcher_dir = Self::get_launcher_dir()?;
        let portable_config = launcher_dir.join("quaternion_config.json");

        if portable_config.exists() {
            return Ok(portable_config);
        }

        if let Some(proj_dirs) = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION) {
            let config_dir = proj_dirs.config_dir();
            fs::create_dir_all(config_dir).context("Failed to create config directory")?;
            Ok(config_dir.join("config.json"))
        } else {
            Ok(portable_config)
        }
    }
}

impl LauncherConfig {
    pub fn load() -> Self {
        if let Ok(config_path) = PathManager::get_config_path() {
            if config_path.exists() {
                if let Ok(content) = fs::read_to_string(&config_path) {
                    if let Ok(config) = serde_json::from_str::<LauncherConfig>(&content) {
                        return config;
                    }
                }
            } else {
                let default_cfg = LauncherConfig::default();
                let _ = default_cfg.save_to(&config_path);
                return default_cfg;
            }
        }
        LauncherConfig::default()
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}
