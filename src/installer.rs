use anyhow::{Context, Result};
use colored::*;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub struct ProtocolInstaller;

impl ProtocolInstaller {
    /// Register the launcher as the OS handler for the polytoria:// scheme
    pub fn register() -> Result<()> {
        let current_exe = env::current_exe().context("Failed to get launcher executable path")?;

        #[cfg(target_os = "windows")]
        {
            Self::register_windows(&current_exe)?;
        }

        #[cfg(target_os = "macos")]
        {
            Self::register_macos(&current_exe)?;
        }

        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            Self::register_linux(&current_exe)?;
        }

        println!(
            "{} Successfully registered 'polytoria://' protocol to '{}'",
            "[✓]".green().bold(),
            current_exe.display()
        );

        Ok(())
    }

    /// Unregister polytoria:// protocol handler
    pub fn unregister() -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            Self::unregister_windows()?;
        }

        #[cfg(target_os = "macos")]
        {
            Self::unregister_macos()?;
        }

        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            Self::unregister_linux()?;
        }

        println!(
            "{} Successfully unregistered 'polytoria://' protocol handler.",
            "[✓]".green().bold()
        );

        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn register_windows(exe_path: &Path) -> Result<()> {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let classes = hkcu.open_subkey_with_flags("Software\\Classes", KEY_ALL_ACCESS)?;

        let scheme = "polytoria";
        let command_str = format!("\"{}\" \"%1\"", exe_path.to_string_lossy());

        let (key, _) = classes.create_subkey(scheme)?;
        key.set_value("", &"URL:Polytoria Protocol")?;
        key.set_value("URL Protocol", &"")?;

        let (cmd_key, _) = classes.create_subkey(format!("{}\\shell\\open\\command", scheme))?;
        cmd_key.set_value("", &command_str)?;

        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn unregister_windows() -> Result<()> {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(classes) = hkcu.open_subkey_with_flags("Software\\Classes", KEY_ALL_ACCESS) {
            let _ = classes.delete_subkey_all("polytoria");
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn register_macos(exe_path: &Path) -> Result<()> {
        let home = env::var("HOME").unwrap_or_else(|_| "/Users/Shared".to_string());
        let app_bundle = PathBuf::from(home).join("Applications/Quaternion Launcher.app");
        let contents_dir = app_bundle.join("Contents");
        let macos_dir = contents_dir.join("MacOS");

        fs::create_dir_all(&macos_dir)?;

        // Write Info.plist declaring CFBundleURLTypes for polytoria
        let info_plist_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>quaternion-launcher</string>
    <key>CFBundleIdentifier</key>
    <string>com.quaternion.launcher</string>
    <key>CFBundleName</key>
    <string>Quaternion Launcher</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleVersion</key>
    <string>1.0.0</string>
    <key>CFBundleURLTypes</key>
    <array>
        <dict>
            <key>CFBundleURLName</key>
            <string>Polytoria Protocol</string>
            <key>CFBundleURLSchemes</key>
            <array>
                <string>polytoria</string>
            </array>
        </dict>
    </array>
</dict>
</plist>"#;

        fs::write(contents_dir.join("Info.plist"), info_plist_content)?;

        // Copy or link the binary to Contents/MacOS/quaternion-launcher
        let target_binary = macos_dir.join("quaternion-launcher");
        if target_binary.exists() {
            let _ = fs::remove_file(&target_binary);
        }
        fs::copy(exe_path, &target_binary)?;

        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&target_binary, fs::Permissions::from_mode(0o755));

        // Register with LaunchServices
        let lsregister_paths = [
            "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister",
            "/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister",
        ];

        for lsregister in &lsregister_paths {
            if Path::new(lsregister).exists() {
                let _ = std::process::Command::new(lsregister)
                    .args(["-f", &app_bundle.to_string_lossy()])
                    .output();
                break;
            }
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn unregister_macos() -> Result<()> {
        let home = env::var("HOME").unwrap_or_else(|_| "/Users/Shared".to_string());
        let app_bundle = PathBuf::from(home).join("Applications/Quaternion Launcher.app");
        if app_bundle.exists() {
            let lsregister_paths = [
                "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister",
                "/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister",
            ];
            for lsregister in &lsregister_paths {
                if Path::new(lsregister).exists() {
                    let _ = std::process::Command::new(lsregister)
                        .args(["-u", &app_bundle.to_string_lossy()])
                        .output();
                    break;
                }
            }
            let _ = fs::remove_dir_all(app_bundle);
        }
        Ok(())
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    fn register_linux(exe_path: &Path) -> Result<()> {
        let home = env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        let apps_dir = PathBuf::from(home).join(".local/share/applications");
        fs::create_dir_all(&apps_dir)?;

        let desktop_file = apps_dir.join("quaternion-launcher.desktop");
        let desktop_content = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Quaternion Launcher\n\
             Comment=Custom Polytoria and Quaternion Game Launcher\n\
             Exec={} %u\n\
             Terminal=false\n\
             Categories=Game;\n\
             MimeType=x-scheme-handler/polytoria;\n",
            exe_path.display()
        );

        fs::write(&desktop_file, desktop_content)?;

        // Update desktop MIME associations via xdg-mime if available
        let _ = std::process::Command::new("xdg-mime")
            .args(["default", "quaternion-launcher.desktop", "x-scheme-handler/polytoria"])
            .output();

        let _ = std::process::Command::new("update-desktop-database")
            .arg(&apps_dir)
            .output();

        Ok(())
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    fn unregister_linux() -> Result<()> {
        let home = env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        let desktop_file = PathBuf::from(home).join(".local/share/applications/quaternion-launcher.desktop");
        if desktop_file.exists() {
            fs::remove_file(desktop_file)?;
        }
        Ok(())
    }
}
