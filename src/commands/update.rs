use anyhow::{Context, Result};
use bollard::Docker;

use crate::app;
use crate::data;

use super::Reporter;
use super::install::pull_changed;

/// Update an app to the latest version of its image.
pub async fn update(docker: &Docker, name: &str, report: Reporter<'_>) -> Result<String> {
    let app = app::find(docker, name).await?;

    report(&format!("Pulling {}", app.settings.image));
    if !pull_changed(docker, &app.settings.image, report).await? {
        return Ok(format!("{} is already up to date", app.settings.name));
    }

    // Recreate the container from its stored settings, keeping the data.
    let container_name = app.container_name();
    let options = bollard::query_parameters::RemoveContainerOptionsBuilder::new()
        .force(true)
        .build();
    docker
        .remove_container(&app.container_id, Some(options))
        .await
        .context("failed to remove the old container")?;

    let create_options = bollard::query_parameters::CreateContainerOptionsBuilder::new()
        .name(&container_name)
        .build();
    docker
        .create_container(Some(create_options), data::with_volumes(&app.settings))
        .await
        .context("failed to recreate the container")?;
    docker
        .start_container(&container_name, None)
        .await
        .context("failed to start the new container")?;

    Ok(format!("Updated {}", app.settings.name))
}
