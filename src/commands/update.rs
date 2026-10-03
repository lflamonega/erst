use anyhow::Result;
use bollard::Docker;

use crate::app;

use super::Reporter;
use super::install::pull_changed;
use super::recreate::{data_note, recreate};

/// Update an app to the latest version of its image.
pub async fn update(docker: &Docker, name: &str, report: Reporter<'_>) -> Result<String> {
    let app = app::find(docker, name).await?;

    report(&format!("Pulling {}", app.settings.image));
    if !pull_changed(docker, &app.settings.image, report).await? {
        return Ok(format!("{} is already up to date", app.settings.name));
    }

    recreate(docker, &app.settings, Some(&app.container_id)).await?;

    let mut message = format!("Updated {}", app.settings.name);
    if let Some(note) = data_note(&app.settings) {
        message.push_str(&format!("\n{note}"));
    }
    Ok(message)
}
