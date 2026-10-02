use anyhow::{Context, Result};
use bollard::Docker;
use bollard::models::{ContainerCreateBody, Mount, MountType, VolumeCreateRequest};

use crate::app::{self, AppSettings, LABEL};

/// Name of the named volume backing one of an app's data paths.
pub fn volume_name(app_name: &str, index: usize) -> String {
    format!("erst-data-{app_name}-{index}")
}

/// Paths the image declares as volumes, i.e. where it keeps its data.
///
/// Using the image's own declarations is what makes this generic: postgres
/// stores data in `/var/lib/postgresql/data`, redis in `/data`, and erst does
/// not need to know either.
pub async fn image_data_paths(docker: &Docker, image: &str) -> Result<Vec<String>> {
    let info = docker
        .inspect_image(&with_default_tag(image))
        .await
        .with_context(|| format!("failed to inspect image {image}"))?;

    Ok(info
        .config
        .and_then(|config| config.volumes)
        .unwrap_or_default())
}

/// Create the named volumes backing an app's data paths.
pub async fn ensure_volumes(docker: &Docker, settings: &AppSettings) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for index in 0..settings.data_paths.len() {
        let name = volume_name(&settings.name, index);
        let labels = std::collections::HashMap::from([(LABEL.to_string(), settings.name.clone())]);

        docker
            .create_volume(VolumeCreateRequest {
                name: Some(name.clone()),
                labels: Some(labels),
                ..Default::default()
            })
            .await
            .with_context(|| format!("failed to create data volume `{name}`"))?;

        names.push(name);
    }
    Ok(names)
}

/// Delete every data volume of an app.
pub async fn remove_volumes(docker: &Docker, settings: &AppSettings) -> Result<()> {
    let options = bollard::query_parameters::RemoveVolumeOptionsBuilder::new()
        .force(true)
        .build();
    for index in 0..settings.data_paths.len() {
        let name = volume_name(&settings.name, index);
        docker
            .remove_volume(&name, Some(options.clone()))
            .await
            .with_context(|| format!("failed to remove the data volume `{name}`"))?;
    }
    Ok(())
}

/// Container body for an app whose data lives in named volumes.
pub fn with_volumes(settings: &AppSettings) -> ContainerCreateBody {
    let mut body = app::container_body(settings);

    let mounts: Vec<Mount> = settings
        .data_paths
        .iter()
        .enumerate()
        .map(|(index, path)| Mount {
            typ: Some(MountType::VOLUME),
            source: Some(volume_name(&settings.name, index)),
            target: Some(path.clone()),
            read_only: Some(false),
            ..Default::default()
        })
        .collect();

    if !mounts.is_empty() {
        body.host_config.get_or_insert_default().mounts = Some(mounts);
    }
    body
}

/// Add `:latest` to references that carry no tag or digest.
fn with_default_tag(reference: &str) -> String {
    if reference.contains(':') || reference.contains('@') {
        return reference.to_string();
    }
    format!("{reference}:latest")
}
