use anyhow::{Context, Result};
use bollard::Docker;

use crate::app;

/// Start a stopped app.
pub async fn start(docker: &Docker, name: &str) -> Result<String> {
    let app = app::find(docker, name).await?;
    docker
        .start_container(&app.container_id, None)
        .await
        .context("failed to start app")?;
    Ok(format!("Started {}", app.settings.name))
}

/// Stop a running app.
pub async fn stop(docker: &Docker, name: &str) -> Result<String> {
    let app = app::find(docker, name).await?;
    let options = bollard::query_parameters::StopContainerOptionsBuilder::new().build();
    docker
        .stop_container(&app.container_id, Some(options))
        .await
        .context("failed to stop app")?;
    Ok(format!("Stopped {}", app.settings.name))
}

/// Restart an app.
pub async fn restart(docker: &Docker, name: &str) -> Result<String> {
    let app = app::find(docker, name).await?;
    let options = bollard::query_parameters::RestartContainerOptionsBuilder::new().build();
    docker
        .restart_container(&app.container_id, Some(options))
        .await
        .context("failed to restart app")?;
    Ok(format!("Restarted {}", app.settings.name))
}
