use anyhow::{Context, Result};
use bollard::Docker;

use crate::app::{self, AppSettings};
use crate::data;

/// What an app loses when its container is rebuilt.
///
/// Data in the declared volumes survives; anything the app wrote anywhere else
/// was living in the container and goes away with it, and there is no volume
/// to carry it. Worth saying out loud, because the loss is silent otherwise.
pub fn data_note(settings: &AppSettings) -> Option<String> {
    settings.data_paths.is_empty().then(|| {
        format!(
            "note: {} declares no volumes, so anything it wrote inside the \
             container is replaced",
            settings.name
        )
    })
}

/// Replace an app's container from its settings, keeping the data volumes.
///
/// A container's settings can only be set when it is created, so updating an
/// image or changing a resource limit means building a new one.
pub async fn recreate(
    docker: &Docker,
    settings: &AppSettings,
    existing: Option<&str>,
) -> Result<()> {
    let container_name = app::container_name(&settings.name);

    if let Some(container_id) = existing {
        let options = bollard::query_parameters::RemoveContainerOptionsBuilder::new()
            .force(true)
            .build();
        docker
            .remove_container(container_id, Some(options))
            .await
            .context("failed to remove the old container")?;
    }

    let options = bollard::query_parameters::CreateContainerOptionsBuilder::new()
        .name(&container_name)
        .build();
    docker
        .create_container(Some(options), data::with_volumes(settings))
        .await
        .with_context(|| format!("failed to create container `{container_name}`"))?;

    docker
        .start_container(&container_name, None)
        .await
        .with_context(|| format!("failed to start container `{container_name}`"))?;

    Ok(())
}
