use anyhow::Result;
use bollard::Docker;

use crate::app;
use crate::data;

/// Remove an app, optionally dropping its data.
pub async fn remove(docker: &Docker, name: &str, remove_data: bool) -> Result<()> {
    let app = app::find(docker, name).await?;
    app::remove(docker, &app, remove_data).await?;

    if remove_data {
        data::remove_volumes(docker, &app.settings).await?;
        println!("Removed {} and its data", app.settings.name);
    } else {
        println!("Removed {}", app.settings.name);
        if !app.settings.data_paths.is_empty() {
            println!("  Data was kept. Pass --remove-data to delete it too.");
        }
    }
    Ok(())
}
