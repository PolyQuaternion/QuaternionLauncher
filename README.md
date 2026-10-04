# Quaternion Launcher

A fast, lightweight bootstrapper, auto-updater, and protocol handler for **Polytoria** running with **Quaternion**.

---

## ⚡ Features

- **Blazing Fast**: Native Rust binary (<50ms startup time).
- **Polytoria Protocol Handler**: Registers with the OS specifically for `polytoria://` (`x-scheme-handler/polytoria`) URLs from web browsers.
- **Cross-Platform**: Full support for **Windows** (Registry `HKCU\Software\Classes\polytoria`), **Linux** (`.desktop` & `xdg-mime`), and **macOS** (LaunchServices & `Info.plist` URL Types).
- **Pre-launch Auto-Updater**: Queries the Quaternion server (`/api/v1/launcher/version?os=<os>`), streams `.zip` packages with checksum verification (SHA256), and extracts updates before Godot starts (preventing OS file locks).
- **Portable or Installed**: Works both standalone alongside a `client/` folder or installed into user data directories.

---

## 🚀 Quick Usage

### Register as default OS Protocol Handler
```bash
# Windows
.\quaternion-launcher.exe --register-protocol

# Linux
./quaternion-launcher-linux --register-protocol

# macOS
./quaternion-launcher-mac --register-protocol
```
When you click "Play" on the Polytoria website, the browser routes through this launcher directly.

To unregister:
```bash
quaternion-launcher --unregister-protocol
```

### Manual Launches
```bash
# Launch with a browser URL
quaternion-launcher "polytoria://launch/eyJhbGciOi..."

# Launch with a direct token
quaternion-launcher -t "<jwt_token>"

# Force update check without launching
quaternion-launcher --update

# Pass additional raw Godot arguments
quaternion-launcher "polytoria://launch/..." -- --rendering-driver vulkan --verbose
```

---

## 🛠️ Building

```bash
# Debug & run unit tests
cargo test

# Release build
cargo build --release
```
The optimized executable will be generated at `target/release/quaternion-launcher`.
