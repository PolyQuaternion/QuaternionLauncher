use anyhow::{anyhow, Context, Result};
use colored::*;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::protocol::LaunchTarget;

pub struct GameLauncher;

impl GameLauncher {
    /// Find the client binary inside the given directory
    pub fn find_executable(client_dir: &Path, custom_path: Option<&PathBuf>) -> Result<PathBuf> {
        if let Some(custom) = custom_path {
            if custom.exists() {
                return Ok(custom.clone());
            }
        }

        #[cfg(target_os = "windows")]
        let candidates = [
            "Polytoria.exe",
            "Quaternion.exe",
            "Polytoria.x86_64.exe",
            "client/Polytoria.exe",
        ];

        #[cfg(target_os = "macos")]
        let candidates = [
            "Polytoria.app/Contents/MacOS/Polytoria",
            "Polytoria.arm64",
            "Polytoria.x86_64",
            "Polytoria",
        ];

        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        let candidates = [
            "Polytoria.x86_64",
            "Polytoria",
            "Quaternion.x86_64",
            "Quaternion",
            "client/Polytoria.x86_64",
        ];

        for candidate in &candidates {
            let full_path = client_dir.join(candidate);
            if full_path.exists() {
                return Ok(full_path);
            }
        }

        // If not found, check if there's any executable in client_dir
        if let Ok(entries) = std::fs::read_dir(client_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                #[cfg(target_os = "windows")]
                if path.extension().map_or(false, |ext| ext == "exe") {
                    return Ok(path);
                }
                #[cfg(not(target_os = "windows"))]
                if path.is_file() {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = path.metadata() {
                        if meta.permissions().mode() & 0o111 != 0 {
                            return Ok(path);
                        }
                    }
                }
            }
        }

        Err(anyhow!(
            "Could not find Polytoria executable in '{}'. Run with --update or place client files there.",
            client_dir.display()
        ))
    }

    /// Launch Godot client with specified target arguments
    pub fn launch(
        client_dir: &Path,
        executable_path: &Path,
        target: &LaunchTarget,
        extra_args: &[String],
        detach: bool,
    ) -> Result<()> {
        let mut cmd = Command::new(executable_path);
        cmd.current_dir(client_dir);

        let mut applied_args = Vec::new();

        if let Some(ref token) = target.token {
            cmd.arg("--token").arg(token);
            applied_args.push(format!("--token {}", token));
        }

        if target.is_creator {
            cmd.arg("--creator");
            applied_args.push("--creator".to_string());
        }

        for arg in &target.extra_godot_args {
            cmd.arg(arg);
            applied_args.push(arg.clone());
        }

        for arg in extra_args {
            cmd.arg(arg);
            applied_args.push(arg.clone());
        }

        println!(
            "{} Launching client: {} {}",
            "[*]".green().bold(),
            executable_path.display(),
            applied_args.join(" ").dimmed()
        );

        if detach {
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                const DETACHED_PROCESS: u32 = 0x00000008;
                cmd.creation_flags(DETACHED_PROCESS);
            }
            cmd.stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .context("Failed to spawn Godot game process")?;
            println!("{} Client started detached.", "[✓]".green());
        } else {
            let mut child = cmd.spawn().context("Failed to spawn Godot game process")?;
            let status = child.wait().context("Failed while waiting for game process")?;
            println!("{} Client exited with code: {:?}", "[*]".cyan(), status.code());
        }

        Ok(())
    }
}
