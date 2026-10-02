use anyhow::{Context, Result};
use bollard::Docker;
use futures::StreamExt;

use crate::app;
use crate::catalog;

/// Install an app from the catalog or from any container image reference.
pub async fn install(
    docker: &Docker,
    reference: &str,
    ports: &[String],
    env: &[String],
    host: Option<String>,
) -> Result<()> {
    let mut settings = match catalog::find(reference) {
        Some(entry) => entry.settings(ports, env)?,
        None => catalog::image_settings(reference, ports, env)?,
    };
    settings.host = host;

    let container_name = app::container_name(&settings.name);
    if app::find(docker, &settings.name).await.is_ok() {
        anyhow::bail!("app `{}` is already installed", settings.name);
    }

    println!("Installing {} ({})", settings.name, settings.image);
    pull(docker, &settings.image).await?;

    let options = bollard::query_parameters::CreateContainerOptionsBuilder::new()
        .name(&container_name)
        .build();
    docker
        .create_container(Some(options), app::container_body(&settings))
        .await
        .with_context(|| format!("failed to create container `{container_name}`"))?;

    docker
        .start_container(&container_name, None)
        .await
        .with_context(|| format!("failed to start container `{container_name}`"))?;

    println!("Installed {} (container {})", settings.name, container_name);
    if let Some(host) = &settings.host {
        println!("  Host: {host}");
    }
    for url in settings.urls() {
        println!("  Open {url}");
    }
    Ok(())
}

/// Pull an image without printing progress.
pub async fn pull(docker: &Docker, image: &str) -> Result<()> {
    pull_changed(docker, image).await?;
    Ok(())
}

/// Pull an image, printing progress. Returns whether the local image changed.
pub async fn pull_changed(docker: &Docker, reference: &str) -> Result<bool> {
    // A reference without a tag would pull every tag of the repository; pin to
    // `latest` so only the default tag is fetched.
    let image = with_default_tag(reference);
    let before = docker.inspect_image(&image).await.ok().map(|info| info.id);

    let options = bollard::query_parameters::CreateImageOptionsBuilder::new()
        .from_image(&image)
        .build();
    let mut stream = docker.create_image(Some(options), None, None);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if let Some(status) = chunk.status {
            if is_layer_progress(&status) {
                continue;
            }
            println!("  {status}");
        } else if let Some(error) = chunk.error_detail {
            anyhow::bail!(
                "failed to pull {image}: {}",
                error.message.unwrap_or_default()
            );
        }
    }

    let after = docker.inspect_image(&image).await.ok().map(|info| info.id);
    Ok(before != after)
}

/// Add `:latest` to references that carry no tag or digest.
fn with_default_tag(reference: &str) -> String {
    if reference.contains(':') || reference.contains('@') {
        return reference.to_string();
    }
    format!("{reference}:latest")
}

/// Layer-by-layer progress lines: the summary lines are the useful ones.
fn is_layer_progress(status: &str) -> bool {
    matches!(
        status,
        "Pulling fs layer"
            | "Downloading"
            | "Download complete"
            | "Extracting"
            | "Pull complete"
            | "Verifying Checksum"
            | "Already exists"
    )
}
