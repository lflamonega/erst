use anyhow::Result;
use bollard::Docker;

use crate::app;

/// Remove an app, optionally dropping its data.
pub async fn remove(docker: &Docker, name: &str, remove_data: bool) -> Result<()> {
    let app = app::find(docker, name).await?;
    app::remove(docker, &app, remove_data).await?;
    println!("Removed {}", app.settings.name);
    if !remove_data {
        println!("  Data was kept. Pass --remove-data to delete it too.");
    }
    Ok(())
}
