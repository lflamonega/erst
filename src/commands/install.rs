use anyhow::{Context, Result};
use bollard::Docker;
use futures::StreamExt;
use std::time::Duration;

use crate::app;
use crate::catalog;
use crate::data;

/// How long to wait before checking whether the container survived startup.
const STARTUP_GRACE: Duration = Duration::from_secs(2);

/// Restarts after this grace period mean the container is crash-looping
/// instead of starting up slowly.
const CRASH_RESTARTS: i64 = 2;

/// Install an app from the catalog or from any container image reference.
pub async fn install(
    docker: &Docker,
    reference: &str,
    ports: &[String],
    env: &[String],
    host: Option<String>,
    no_harden: bool,
) -> Result<()> {
    let mut settings = match catalog::find(reference) {
        Some(entry) => entry.settings(ports, env)?,
        None => catalog::image_settings(reference, ports, env)?,
    };
    settings.host = host;
    settings.harden = !no_harden;

    let container_name = app::container_name(&settings.name);
    if app::find(docker, &settings.name).await.is_ok() {
        anyhow::bail!("app `{}` is already installed", settings.name);
    }

    println!("Installing {} ({})", settings.name, settings.image);
    pull(docker, &settings.image).await?;
    data::ensure_volumes(docker, &settings).await?;

    // Record who the image intends to run as, so `list` can surface it.
    settings.user = image_user(docker, &settings.image).await?;
    settings.data_paths = data::image_data_paths(docker, &settings.image).await?;
    if !settings.data_paths.is_empty() {
        println!(
            "  Data: {} (kept in {} named volume/s)",
            settings.data_paths.join(", "),
            settings.data_paths.len()
        );
    }
    if settings.runs_as_root() && settings.harden {
        eprintln!(
            "warning: {} runs as root inside the container. \
             Only install apps you trust.",
            settings.name
        );
    }

    let options = bollard::query_parameters::CreateContainerOptionsBuilder::new()
        .name(&container_name)
        .build();
    docker
        .create_container(Some(options), data::with_volumes(&settings))
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

    warn_if_crashed(docker, &container_name, &settings).await?;
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

/// The user declared by the image; empty when the image runs as root.
async fn image_user(docker: &Docker, image: &str) -> Result<String> {
    let info = docker
        .inspect_image(&with_default_tag(image))
        .await
        .with_context(|| format!("failed to inspect image {image}"))?;
    Ok(info
        .config
        .and_then(|config| config.user)
        .unwrap_or_default())
}

/// If the container died right after starting, show why instead of pretending
/// the install succeeded.
async fn warn_if_crashed(
    docker: &Docker,
    container: &str,
    settings: &app::AppSettings,
) -> Result<()> {
    tokio::time::sleep(STARTUP_GRACE).await;

    let inspect = docker.inspect_container(container, None).await?;
    let state = inspect.state.unwrap_or_default();

    // With `restart: unless-stopped` a crashed container never stays down, so a
    // run of restarts is the signal that it is failing rather than starting.
    let restarts = inspect.restart_count.unwrap_or(0);
    let crashed = !state.running.unwrap_or(false) || restarts >= CRASH_RESTARTS;
    if !crashed {
        return Ok(());
    }

    let detail = state
        .error
        .or_else(|| state.status.map(|s| format!("{s:?}")))
        .unwrap_or_else(|| "unknown status".to_string());
    if restarts >= CRASH_RESTARTS {
        eprintln!(
            "\n{} is restarting repeatedly ({detail}). Last log lines:",
            settings.name
        );
    } else {
        eprintln!(
            "\n{} exited immediately ({detail}). Last log lines:",
            settings.name
        );
    }
    let options = bollard::query_parameters::LogsOptionsBuilder::new()
        .stdout(true)
        .stderr(true)
        .tail("20")
        .build();
    let mut stream = docker.logs(container, Some(options));
    while let Some(chunk) = stream.next().await {
        eprint!("{}", chunk?);
    }

    if settings.runs_as_root() {
        eprintln!(
            "\nIf this app needs to escalate privileges on start, retry with: \
             erst install {} --no-harden",
            settings.image
        );
    } else {
        eprintln!("\nInspect the app with: erst logs {}", settings.name);
    }
    Ok(())
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
