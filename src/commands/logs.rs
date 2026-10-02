use anyhow::Result;
use bollard::Docker;
use futures::StreamExt;

use crate::app;

/// Show the logs of an app.
pub async fn logs(docker: &Docker, name: &str, follow: bool, tail: u64) -> Result<()> {
    let app = app::find(docker, name).await?;
    let options = bollard::query_parameters::LogsOptionsBuilder::new()
        .stdout(true)
        .stderr(true)
        .follow(follow)
        .tail(&tail.to_string())
        .build();

    let mut stream = docker.logs(&app.container_id, Some(options));
    while let Some(chunk) = stream.next().await {
        print!("{}", chunk?);
    }
    Ok(())
}

/// Read the tail of an app's logs into lines, for the TUI log view.
pub async fn tail_logs(docker: &Docker, name: &str, tail: u64) -> Result<Vec<String>> {
    let app = app::find(docker, name).await?;
    let options = bollard::query_parameters::LogsOptionsBuilder::new()
        .stdout(true)
        .stderr(true)
        .follow(false)
        .tail(&tail.to_string())
        .build();

    let mut stream = docker.logs(&app.container_id, Some(options));
    let mut lines = Vec::new();
    while let Some(chunk) = stream.next().await {
        lines.extend(
            String::from_utf8_lossy(&chunk?.into_bytes())
                .lines()
                .map(str::to_string),
        );
    }
    Ok(lines)
}
