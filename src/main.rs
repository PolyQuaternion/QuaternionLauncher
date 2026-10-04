mod config;
mod installer;
mod launcher;
mod protocol;
mod updater;

use anyhow::Result;
use clap::Parser;
use colored::*;
use std::path::PathBuf;

use config::{LauncherConfig, PathManager};
use installer::ProtocolInstaller;
use launcher::GameLauncher;
use protocol::{LaunchTarget, ProtocolParser};
use updater::Updater;

#[derive(Parser, Debug)]
#[command(
    name = "quaternion-launcher",
    author = "Quaternion Developers",
    version = "0.1.0",
    about = "Lightweight bootstrapper, updater, and protocol launcher for Quaternion & Polytoria"
)]
struct Cli {
    /// URL (polytoria://...) or direct launch token passed by browser or command line
    #[arg(value_name = "URI_OR_TOKEN")]
    uri_or_token: Option<String>,

    /// Explicit launch token
    #[arg(short, long)]
    token: Option<String>,

    /// Launch in Creator (Studio) mode
    #[arg(long)]
    creator: bool,

    /// Skip checking for updates before launch
    #[arg(long)]
    no_update: bool,

    /// Force check and download latest update
    #[arg(long)]
    update: bool,

    /// Register this launcher as the default OS protocol handler for polytoria://
    #[arg(long)]
    register_protocol: bool,

    /// Unregister this launcher from OS protocol handlers
    #[arg(long)]
    unregister_protocol: bool,

    /// Custom path to client directory or Godot executable
    #[arg(long)]
    client_path: Option<PathBuf>,

    /// Custom update manifest endpoint URL
    #[arg(long)]
    update_url: Option<String>,

    /// Release channel (e.g. "stable", "beta", "dev")
    #[arg(long)]
    channel: Option<String>,

    /// Launch game detached from terminal
    #[arg(long, default_value_t = false)]
    detach: bool,

    /// Additional raw arguments forwarded to Godot
    #[arg(last = true)]
    godot_args: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // 1. Handle Protocol Registration / Unregistration commands
    if cli.register_protocol {
        return ProtocolInstaller::register();
    }
    if cli.unregister_protocol {
        return ProtocolInstaller::unregister();
    }

    println!(
        "{} {}",
        "✦ Quaternion Bootstrapper".cyan().bold(),
        format!("v{}", env!("CARGO_PKG_VERSION")).dimmed()
    );

    // 2. Load Configuration
    let mut config = LauncherConfig::load();
    if let Some(ref u) = cli.update_url {
        config.update_url = u.clone();
    }
    if let Some(ref ch) = cli.channel {
        config.channel = ch.clone();
    }
    if let Some(ref cp) = cli.client_path {
        config.custom_client_path = Some(cp.clone());
    }

    // 3. Resolve Client Paths
    let client_dir = if let Some(ref custom) = config.custom_client_path {
        if custom.is_dir() {
            custom.clone()
        } else {
            custom
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        }
    } else {
        PathManager::get_client_dir()?
    };

    println!(
        "{} Client directory: {}",
        "[*]".cyan(),
        client_dir.display().to_string().yellow()
    );

    // 4. Parse incoming target (URL or flags)
    let mut target = LaunchTarget {
        token: cli.token,
        is_creator: cli.creator,
        extra_godot_args: cli.godot_args,
    };

    if let Some(ref input) = cli.uri_or_token {
        if input.starts_with("polytoria://") {
            match ProtocolParser::parse_uri(input) {
                Ok(parsed) => {
                    if target.token.is_none() {
                        target.token = parsed.token;
                    }
                    if parsed.is_creator {
                        target.is_creator = true;
                    }
                    target.extra_godot_args.extend(parsed.extra_godot_args);
                }
                Err(err) => {
                    println!("{} Failed to parse URL: {}", "[!]".yellow(), err);
                }
            }
        } else if target.token.is_none() {
            // Positional argument treated as direct token
            target.token = Some(input.clone());
        }
    }

    // 5. Updater Step (Pre-launch)
    let should_update = (config.auto_update && !cli.no_update) || cli.update;
    let updater = Updater::new(client_dir.clone());

    if should_update {
        let _ = updater
            .check_and_update(&config.update_url, &config.channel)
            .await;
    }

    // If --update only was requested with no token/launch intent, we can exit cleanly
    if cli.update && target.token.is_none() && !target.is_creator && cli.uri_or_token.is_none() {
        println!("{} Update completed. Exiting.", "[✓]".green());
        return Ok(());
    }

    // 6. Locate Executable & Launch
    let executable = GameLauncher::find_executable(&client_dir, config.custom_client_path.as_ref())?;
    
    GameLauncher::launch(
        &client_dir,
        &executable,
        &target,
        &config.extra_args,
        cli.detach,
    )?;

    Ok(())
}
