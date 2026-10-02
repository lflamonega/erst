use anyhow::{Context, Result};
use bollard::Docker;

use crate::app;

use super::install::pull_changed;

/// Update an app to the latest version of its image.
pub async fn update(docker: &Docker, name: &str) -> Result<()> {
    let app = app::find(docker, name).await?;

    println!("Pulling {} ({})", app.settings.name, app.settings.image);
    if !pull_changed(docker, &app.settings.image).await? {
        println!("{} is already up to date", app.settings.name);
        return Ok(());
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
        .create_container(Some(create_options), app::container_body(&app.settings))
        .await
        .context("failed to recreate the container")?;
    docker
        .start_container(&container_name, None)
        .await
        .context("failed to start the new container")?;

    println!("Updated {}", app.settings.name);
    Ok(())
}
