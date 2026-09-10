use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateDownloadProgress {
    downloaded: u64,
    total: Option<u64>,
}

/// Download the available update, install it, then restart the app.
///
/// On macOS the updater replaces the `.app` bundle in place. Running install and
/// restart on the Rust side avoids relying on the old WebView after replacement.
#[tauri::command]
pub async fn install_update_and_restart(app: AppHandle) -> Result<bool, String> {
    let updater = app
        .updater_builder()
        .build()
        .map_err(|error| format!("Failed to initialize updater: {error}"))?;

    let Some(update) = updater
        .check()
        .await
        .map_err(|error| format!("Failed to check for updates: {error}"))?
    else {
        return Ok(false);
    };

    let progress_handle = app.clone();
    let mut downloaded: u64 = 0;
    let bytes = update
        .download(
            move |chunk_len, content_len| {
                downloaded = downloaded.saturating_add(chunk_len as u64);
                let _ = progress_handle.emit(
                    "update-download-progress",
                    UpdateDownloadProgress {
                        downloaded,
                        total: content_len,
                    },
                );
            },
            || {},
        )
        .await
        .map_err(|error| format!("Failed to download update: {error}"))?;

    #[cfg(target_os = "windows")]
    {
        // Windows install launches the installer and exits the process.
        update
            .install(bytes)
            .map_err(|error| format!("Failed to install update: {error}"))?;
        return Ok(true);
    }

    #[cfg(not(target_os = "windows"))]
    {
        update
            .install(bytes)
            .map_err(|error| format!("Failed to install update: {error}"))?;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        app.restart();
    }
}
