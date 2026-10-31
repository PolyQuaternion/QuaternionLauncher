use crate::updater::{PendingUpdate, UpdateProgressObserver, Updater};
use anyhow::{anyhow, Result};
use std::sync::Arc;

slint::slint! {
    export component UpdateDialog inherits Window {
        in property <string> title_text: "Updating Quaternion";
        in property <string> subtitle_text: "Please wait while files are being downloaded";
        in property <string> status_text: "Preparing update...";
        in property <string> progress_text: "0%";
        in property <float> progress_value: 0.0;

        title: "Quaternion Launcher";
        width: 440px;
        height: 140px;
        background: rgb(24, 25, 30);

        VerticalLayout {
            padding-left: 20px;
            padding-right: 20px;
            padding-top: 16px;
            padding-bottom: 16px;
            spacing: 10px;
            alignment: center;

            HorizontalLayout {
                spacing: 8px;
                alignment: space-between;

                VerticalLayout {
                    spacing: 2px;
                    Text {
                        text: root.title_text;
                        font-size: 15px;
                        font-weight: 700;
                        color: #ffffff;
                    }
                    Text {
                        text: root.subtitle_text;
                        font-size: 11px;
                        color: #8f929b;
                    }
                }

                Text {
                    text: root.progress_text;
                    font-size: 14px;
                    font-weight: 600;
                    color: #00d2ff;
                    vertical-alignment: center;
                }
            }

            // Progress bar
            Rectangle {
                height: 8px;
                border-radius: 4px;
                background: #282a32;

                Rectangle {
                    x: 0;
                    width: parent.width * Math.max(0.0, Math.min(1.0, root.progress_value));
                    height: parent.height;
                    border-radius: 4px;
                    background: #00b4d8;
                }
            }

            HorizontalLayout {
                Text {
                    text: root.status_text;
                    font-size: 11px;
                    color: #a0a4ad;
                    overflow: elide;
                }
            }
        }
    }
}

pub struct SlintUpdateNotifier {
    ui_handle: slint::Weak<UpdateDialog>,
}

impl SlintUpdateNotifier {
    pub fn new(ui_handle: slint::Weak<UpdateDialog>) -> Self {
        Self { ui_handle }
    }

    pub fn set_status(&self, status: String) {
        let _ = self.ui_handle.upgrade_in_event_loop(move |ui| {
            ui.set_status_text(status.into());
        });
    }

    pub fn set_progress(&self, fraction: f32, status: String) {
        let percent_str = format!("{:.0}%", (fraction.clamp(0.0, 1.0) * 100.0));
        let _ = self.ui_handle.upgrade_in_event_loop(move |ui| {
            ui.set_progress_value(fraction);
            ui.set_progress_text(percent_str.into());
            ui.set_status_text(status.into());
        });
    }

    pub fn set_versions(&self, title: String, subtitle: String) {
        let _ = self.ui_handle.upgrade_in_event_loop(move |ui| {
            ui.set_title_text(title.into());
            ui.set_subtitle_text(subtitle.into());
        });
    }

    pub fn close(&self) {
        let _ = slint::invoke_from_event_loop(move || {
            let _ = slint::quit_event_loop();
        });
    }
}

impl UpdateProgressObserver for SlintUpdateNotifier {
    fn on_status(&self, message: &str) {
        self.set_status(message.to_string());
    }

    fn on_progress(&self, fraction: f32, message: &str) {
        self.set_progress(fraction, message.to_string());
    }
}

pub fn run_update_dialog(
    updater: Arc<Updater>,
    pending: PendingUpdate,
    channel: String,
) -> Result<()> {
    let dialog = UpdateDialog::new()?;
    let ui_weak = dialog.as_weak();
    let notifier = Arc::new(SlintUpdateNotifier::new(ui_weak));

    let target_version = pending.manifest.version.clone();
    let current_ver = pending
        .current_client_version
        .clone()
        .unwrap_or_else(|| "none".to_string());

    notifier.set_versions(
        "Updating Quaternion".to_string(),
        format!("v{} → v{} ({})", current_ver, target_version, channel),
    );
    notifier.set_status("Starting download...".to_string());

    let notifier_clone = notifier.clone();
    let (tx, rx) = std::sync::mpsc::channel();

    let pending_clone = pending.clone();
    let channel_clone = channel.clone();
    let updater_clone = updater.clone();

    tokio::spawn(async move {
        let result = updater_clone
            .apply_update(&pending_clone, &channel_clone, Some(notifier_clone.clone()))
            .await;
        let _ = tx.send(result);

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        notifier_clone.close();
    });

    dialog.run()?;

    match rx.recv() {
        Ok(res) => res.map(|_| ()),
        Err(_) => Err(anyhow!("Update dialog was closed before finishing")),
    }
}