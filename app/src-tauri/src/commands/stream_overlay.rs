//! Commands for the generic localhost web overlay.

use baras_types::WEB_OVERLAY_URL;
use tauri::State;

use crate::overlay::{OverlayCommand, SharedOverlayState};
use crate::service::ServiceHandle;
use crate::stream_overlay::WebOverlayServer;

/// Enable or disable the loopback web overlay and persist the setting.
#[tauri::command]
pub async fn set_web_overlay_enabled(
    enabled: bool,
    server: State<'_, WebOverlayServer>,
    overlay_state: State<'_, SharedOverlayState>,
    service: State<'_, ServiceHandle>,
) -> Result<String, String> {
    if enabled {
        server.start().await?;

        // Existing overlays may not render again until their data changes, so
        // publish their current buffers immediately when the server is enabled.
        let senders = {
            let state = overlay_state.lock().map_err(|error| error.to_string())?;
            state
                .all_overlays()
                .into_iter()
                .map(|(_, sender)| sender.clone())
                .collect::<Vec<_>>()
        };
        for sender in senders {
            let _ = sender.send(OverlayCommand::CaptureFrame).await;
        }
    } else {
        server.stop().await;
    }

    let mut config = service.config().await;
    config.web_overlay_enabled = enabled;
    service.update_config(config).await?;

    Ok(WEB_OVERLAY_URL.to_string())
}
