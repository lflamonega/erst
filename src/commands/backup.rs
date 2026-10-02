use anyhow::{Context, Result, bail};
use bollard::Docker;
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, DownloadFromContainerOptionsBuilder,
    RemoveContainerOptionsBuilder, UploadToContainerOptionsBuilder,
};
use futures::StreamExt;
use std::io::Write;
use std::path::Path;

use crate::app::{self, AppSettings};
use crate::data;

/// Layout of a backup archive: the app settings, then one tar per data volume.
const MANIFEST: &str = "erst.json";

fn volume_entry(index: usize) -> String {
    format!("volumes/{index}.tar")
}

fn decoder(file: std::fs::File) -> flate2::read::MultiGzDecoder<std::fs::File> {
    flate2::read::MultiGzDecoder::new(file)
}

fn encoder(file: std::fs::File) -> flate2::write::GzEncoder<std::fs::File> {
    flate2::write::GzEncoder::new(file, flate2::Compression::default())
}

/// Write a tar.gz of an app's data volumes to `destination`.
pub async fn backup(docker: &Docker, name: &str, destination: &Path) -> Result<()> {
    let app = app::find(docker, name).await?;
    if app.settings.data_paths.is_empty() {
        bail!("`{name}` declares no data to back up");
    }

    let file = std::fs::File::create(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    let mut archive = tar::Builder::new(encoder(file));

    append(
        &mut archive,
        MANIFEST,
        &serde_json::to_vec_pretty(&app.settings)?,
    )?;

    for (index, path) in app.settings.data_paths.iter().enumerate() {
        println!("  Backing up {path}");
        let reader = volume_io_container(docker, &app.settings).await?;
        let options = DownloadFromContainerOptionsBuilder::new()
            .path(path)
            .build();
        let bytes = collect(docker.download_from_container(&reader, Some(options))).await?;
        drop_container(docker, &reader).await;

        append(&mut archive, &volume_entry(index), &bytes)?;
    }

    let encoder = archive
        .into_inner()
        .context("failed to finish the backup archive")?;
    encoder
        .finish()
        .context("failed to finish the backup file")?;

    println!("Backed up {} to {}", name, destination.display());
    Ok(())
}

/// Recreate an app from a backup archive.
pub async fn restore(docker: &Docker, source: &Path) -> Result<()> {
    let Backup { settings, volumes } = read(source)?;

    if app::find(docker, &settings.name).await.is_ok() {
        bail!("`{}` is already installed; remove it first", settings.name);
    }

    println!("Restoring {} ({})", settings.name, settings.image);
    crate::commands::install::pull(docker, &settings.image).await?;
    data::ensure_volumes(docker, &settings).await?;

    for (index, path) in settings.data_paths.iter().enumerate() {
        let bytes = volumes
            .get(index)
            .with_context(|| format!("the archive is missing {}", volume_entry(index)))?;
        let reader = volume_io_container(docker, &settings).await?;

        let options = UploadToContainerOptionsBuilder::new()
            .path(parent_of(path))
            .build();
        docker
            .upload_to_container(
                &reader,
                Some(options),
                bollard::body_stream(once(bytes.clone())),
            )
            .await
            .with_context(|| format!("failed to upload data to {path}"))?;
        drop_container(docker, &reader).await;

        println!("  Restored {path}");
    }

    let container_name = app::container_name(&settings.name);
    docker
        .create_container(
            Some(
                CreateContainerOptionsBuilder::new()
                    .name(&container_name)
                    .build(),
            ),
            data::with_volumes(&settings),
        )
        .await
        .with_context(|| format!("failed to create container `{container_name}`"))?;
    docker
        .start_container(&container_name, None)
        .await
        .with_context(|| format!("failed to start container `{container_name}`"))?;

    println!("Restored {} (container {})", settings.name, container_name);
    for url in settings.urls() {
        println!("  Open {url}");
    }
    Ok(())
}

/// A short-lived container used only to read from or write to a data volume.
async fn volume_io_container(docker: &Docker, settings: &AppSettings) -> Result<String> {
    let name = format!("erst-volume-io-{}", settings.name);
    // A leftover from an interrupted backup would make the name unusable.
    docker
        .remove_container(&name, Some(force_remove()))
        .await
        .ok();

    docker
        .create_container(
            Some(CreateContainerOptionsBuilder::new().name(&name).build()),
            data::with_volumes(settings),
        )
        .await
        .with_context(|| format!("failed to create the helper container `{name}`"))?;
    Ok(name)
}

async fn drop_container(docker: &Docker, name: &str) {
    docker
        .remove_container(name, Some(force_remove()))
        .await
        .ok();
}

fn force_remove() -> bollard::query_parameters::RemoveContainerOptions {
    RemoveContainerOptionsBuilder::new().force(true).build()
}

/// A backup archive, read into memory.
struct Backup {
    settings: AppSettings,
    volumes: Vec<Vec<u8>>,
}

/// Read an archive in one pass: the manifest plus every volume tar.
fn read(source: &Path) -> Result<Backup> {
    let file = std::fs::File::open(source)
        .with_context(|| format!("failed to open {}", source.display()))?;
    let mut archive = tar::Archive::new(decoder(file));

    let mut manifest = None;
    let mut volumes = Vec::new();
    let entries = archive
        .entries()
        .context("the backup is not a readable tar archive")?;

    for entry in entries {
        let mut entry = entry.context("the backup archive is corrupt")?;
        let path = entry
            .path()
            .context("the backup archive has an invalid entry name")?
            .to_string_lossy()
            .into_owned();

        let mut bytes = Vec::new();
        std::io::copy(&mut entry, &mut bytes).context("the backup archive is corrupt")?;

        if path == MANIFEST {
            manifest = Some(bytes);
        } else if let Some(index) = path
            .strip_prefix("volumes/")
            .and_then(|n| n.strip_suffix(".tar").and_then(|n| n.parse::<usize>().ok()))
        {
            if volumes.len() <= index {
                volumes.resize_with(index + 1, Vec::new);
            }
            volumes[index] = bytes;
        }
    }

    let manifest = manifest.context("the archive has no erst manifest; is it an erst backup?")?;
    let settings = serde_json::from_slice(&manifest).context("the backup manifest is corrupt")?;
    Ok(Backup { settings, volumes })
}

fn append<W: Write>(archive: &mut tar::Builder<W>, name: &str, content: &[u8]) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_entry_type(tar::EntryType::Regular);
    header
        .set_path(name)
        .context("invalid archive entry name")?;
    header.set_cksum();

    archive
        .append(&header, std::io::Cursor::new(content))
        .with_context(|| format!("failed to write {name} into the backup"))?;
    Ok(())
}

async fn collect<S>(mut stream: S) -> Result<Vec<u8>>
where
    S: futures::Stream<Item = Result<bytes::Bytes, bollard::errors::Error>> + Unpin,
{
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk?);
    }
    Ok(bytes)
}

/// The upload endpoint takes a stream of `bytes::Bytes` chunks.
fn once(bytes: Vec<u8>) -> impl futures::Stream<Item = bytes::Bytes> {
    futures::stream::once(async move { bytes::Bytes::from(bytes) })
}

/// Directory to extract a volume tar into.
///
/// Docker tars a directory including its own name, so a backup of
/// `/var/lib/postgresql/data` holds `data/...` entries and has to be extracted
/// in the parent directory to land back where it came from.
fn parent_of(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => "/",
        Some(index) => &trimmed[..index],
    }
}

#[cfg(test)]
mod tests {
    use super::parent_of;

    #[test]
    fn volume_tars_are_extracted_in_the_parent_directory() {
        assert_eq!(parent_of("/var/lib/postgresql/data"), "/var/lib/postgresql");
        assert_eq!(parent_of("/data"), "/");
        assert_eq!(parent_of("/data/"), "/");
        assert_eq!(parent_of("/a/b/c"), "/a/b");
    }
}
