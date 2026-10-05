use anyhow::Result;
use bollard::Docker;

use crate::app;
use crate::autostart;
use crate::data;
use crate::network;

/// Remove an app, optionally dropping its data.
pub async fn remove(docker: &Docker, name: &str, remove_data: bool) -> Result<String> {
    let app = app::find(docker, name).await?;
    app::remove(docker, &app, remove_data).await?;
    // Also drop the login entry: a unit left behind would point at a name that
    // no longer exists.
    autostart::remove(&app.settings.name).await;

    let mut message = format!("Removed {}", app.settings.name);
    if remove_data {
        data::remove_volumes(docker, &app.settings).await?;
        message.push_str(" and its data");
    } else if !app.settings.data_paths.is_empty() {
        message.push_str("\n  Data was kept. Pass --remove-data to delete it too.");
    }

    // The network belonged to whoever was on it, so it goes only now that the
    // last app on it has. Best effort: the app is already gone, and reporting
    // a failure here would contradict a removal that did happen.
    if let Some(name) = &app.settings.network
        && network::remove_if_empty(docker, name)
            .await
            .unwrap_or(false)
    {
        message.push_str(&format!("\n  The network {name} was removed too."));
    }
    Ok(message)
}
