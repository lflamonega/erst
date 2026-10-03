use anyhow::{Result, bail};
use bollard::Docker;

use crate::app;
use crate::limits;

use super::recreate::{data_note, recreate};

/// Change an app's resource limits.
pub async fn limit(
    docker: &Docker,
    name: &str,
    memory: Option<&str>,
    cpu: Option<&str>,
) -> Result<String> {
    if memory.is_none() && cpu.is_none() {
        bail!("nothing to change: pass --memory or --cpu");
    }

    let mut app = app::find(docker, name).await?;

    if let Some(value) = memory {
        app.settings.memory = limits::memory_value(value)?;
    }
    if let Some(value) = cpu {
        app.settings.cpu_nanos = limits::cpu_value(value)?;
    }

    let mut message = if app.settings.limits() == "-" {
        format!("Removed the limits of {}", app.settings.name)
    } else {
        format!(
            "{} is now limited to {}",
            app.settings.name,
            app.settings.limits()
        )
    };
    if let Some(note) = data_note(&app.settings) {
        message.push_str(&format!("\n{note}"));
    }

    // A container's settings only exist at creation time, so changing them
    // means building a new one from the same settings and volumes.
    recreate(docker, &app.settings, Some(&app.container_id)).await?;
    Ok(message)
}
