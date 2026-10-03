use anyhow::{Context, Result};
use bollard::Docker;
use futures::StreamExt;
use std::time::Duration;

use super::Reporter;
use crate::app::{self, AppSettings};
use crate::catalog::{self, Entry};
use crate::data;
use crate::limits;

/// How long to wait before checking whether the container survived startup.
const STARTUP_GRACE: Duration = Duration::from_secs(2);

/// Restarts after this grace period mean the container is crash-looping
/// instead of starting up slowly.
const CRASH_RESTARTS: i64 = 2;

/// What the user asked for when installing an app.
///
/// Bundled rather than passed as arguments: there are enough knobs that a flat
/// list would stop being readable at the call site.
#[derive(Debug, Default)]
pub struct InstallOptions {
    /// Catalog app name or container image reference.
    pub reference: String,
    /// Overrides for the ports and environment the image declares.
    pub ports: Vec<String>,
    pub env: Vec<String>,
    /// Hostname to access the app under.
    pub host: Option<String>,
    /// Whether hardening applies.
    pub harden: bool,
    /// Memory cap as typed: `512m`, `1g`, a bare `512`, or `unlimited`.
    pub memory: Option<String>,
    /// CPU allowance as typed: `1`, `0.5`, or `unlimited`.
    pub cpu: Option<String>,
}

/// Install an app from the catalog or from any container image reference.
pub async fn install(
    docker: &Docker,
    options: InstallOptions,
    report: Reporter<'_>,
) -> Result<String> {
    let mut settings = match catalog::find(&options.reference) {
        Some(entry) => entry.settings(&options.ports, &options.env)?,
        None => catalog::image_settings(&options.reference, &options.ports, &options.env)?,
    };
    settings.host = options.host;
    settings.harden = options.harden;
    if let Some(value) = &options.memory {
        settings.memory = limits::memory_value(value)?;
    }
    if let Some(value) = &options.cpu {
        settings.cpu_nanos = limits::cpu_value(value)?;
    }

    install_settings(docker, settings, report).await
}

/// Install a catalog entry with its own defaults, used by the dashboard.
pub async fn install_entry(docker: &Docker, entry: &Entry, report: Reporter<'_>) -> Result<String> {
    let options = InstallOptions {
        reference: entry.name.to_string(),
        harden: true,
        ..InstallOptions::default()
    };
    install(docker, options, report).await
}

async fn install_settings(
    docker: &Docker,
    mut settings: AppSettings,
    report: Reporter<'_>,
) -> Result<String> {
    if app::find(docker, &settings.name).await.is_ok() {
        anyhow::bail!("app `{}` is already installed", settings.name);
    }

    report(&format!(
        "Installing {} ({})",
        settings.name, settings.image
    ));
    pull(docker, &settings.image, report).await?;
    data::ensure_volumes(docker, &settings).await?;

    // Record who the image intends to run as, so `list` can surface it.
    settings.user = image_user(docker, &settings.image).await?;
    settings.data_paths = data::image_data_paths(docker, &settings.image).await?;
    if settings.runs_as_root() {
        report(&format!(
            "warning: {} runs as root inside the container. Only install apps you trust.",
            settings.name
        ));
    }

    let container_name = app::container_name(&settings.name);
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

    let mut message = format!("Installed {} ({container_name})", settings.name);
    let limits = settings.limits();
    if limits != "-" {
        message.push_str(&format!(", limited to {limits}"));
    }
    for url in settings.urls() {
        message.push_str(&format!("\n  Open {url}"));
    }
    if let Some(crash) = crash_report(docker, &container_name, &settings).await? {
        message.push_str(&format!("\n{crash}"));
    }

    Ok(message)
}

/// Pull an image, reporting progress.
pub async fn pull(docker: &Docker, image: &str, report: Reporter<'_>) -> Result<()> {
    pull_changed(docker, image, report).await?;
    Ok(())
}

/// Pull an image. Returns whether the local image changed.
pub async fn pull_changed(docker: &Docker, reference: &str, report: Reporter<'_>) -> Result<bool> {
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
            if !is_layer_progress(&status) {
                report(&status);
            }
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

/// Describe a container that exited or crash-looped right after starting.
///
/// With `restart: unless-stopped` a crashed container never stays down, so a run
/// of restarts is the signal that it is failing rather than starting slowly.
async fn crash_report(
    docker: &Docker,
    container: &str,
    settings: &AppSettings,
) -> Result<Option<String>> {
    tokio::time::sleep(STARTUP_GRACE).await;

    let inspect = docker.inspect_container(container, None).await?;
    let state = inspect.state.unwrap_or_default();
    let restarts = inspect.restart_count.unwrap_or(0);
    if state.running.unwrap_or(false) && restarts < CRASH_RESTARTS {
        return Ok(None);
    }

    let detail = state
        .error
        .or_else(|| state.status.map(|status| format!("{status:?}")))
        .unwrap_or_else(|| "unknown status".to_string());

    Ok(Some(if restarts >= CRASH_RESTARTS {
        format!(
            "but {} is restarting repeatedly ({detail}); check `erst logs {}`",
            settings.name, settings.name
        )
    } else {
        format!(
            "but {} exited immediately ({detail}); check `erst logs {}`",
            settings.name, settings.name
        )
    }))
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
