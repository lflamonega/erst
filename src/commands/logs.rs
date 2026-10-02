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
