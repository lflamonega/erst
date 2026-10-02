use anyhow::{Context, Result};
use bollard::Docker;

use crate::app;

/// Start a stopped app.
pub async fn start(docker: &Docker, name: &str) -> Result<()> {
    let app = app::find(docker, name).await?;
    docker
        .start_container(&app.container_id, None)
        .await
        .context("failed to start app")?;
    println!("Started {}", app.settings.name);
    Ok(())
}

/// Stop a running app.
pub async fn stop(docker: &Docker, name: &str) -> Result<()> {
    let app = app::find(docker, name).await?;
    let options = bollard::query_parameters::StopContainerOptionsBuilder::new().build();
    docker
        .stop_container(&app.container_id, Some(options))
        .await
        .context("failed to stop app")?;
    println!("Stopped {}", app.settings.name);
    Ok(())
}

/// Restart an app.
pub async fn restart(docker: &Docker, name: &str) -> Result<()> {
    let app = app::find(docker, name).await?;
    let options = bollard::query_parameters::RestartContainerOptionsBuilder::new().build();
    docker
        .restart_container(&app.container_id, Some(options))
        .await
        .context("failed to restart app")?;
    println!("Restarted {}", app.settings.name);
    Ok(())
}
