use anyhow::Result;
use bollard::Docker;

use crate::app;

/// List installed apps, running ones first.
pub async fn list(docker: &Docker) -> Result<()> {
    let apps = app::list(docker).await?;
    if apps.is_empty() {
        println!("No apps installed. Install one with: erst install <app>");
        return Ok(());
    }

    println!(
        "{:<20} {:<30} {:<12} {:<12} {:<10}",
        "NAME", "IMAGE", "STATUS", "PORTS", "USER"
    );
    for app in sort_by_running(apps) {
        let ports = app
            .settings
            .ports
            .iter()
            .map(|port| port.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "{:<20} {:<30} {:<12} {:<12} {:<10}",
            app.settings.name,
            app.settings.image,
            app.status,
            ports,
            user_column(&app.settings)
        );
    }
    Ok(())
}

/// The container's user, flagged with `!` when it is root.
fn user_column(settings: &app::AppSettings) -> String {
    if settings.runs_as_root() {
        "root!".to_string()
    } else {
        settings.user.trim().to_string()
    }
}

fn sort_by_running(mut apps: Vec<app::InstalledApp>) -> Vec<app::InstalledApp> {
    apps.sort_by(|a, b| {
        b.running
            .cmp(&a.running)
            .then_with(|| a.settings.name.cmp(&b.settings.name))
    });
    apps
}
