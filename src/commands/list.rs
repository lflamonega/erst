use anyhow::Result;
use bollard::Docker;
use futures::future::join_all;

use crate::app::{self, InstalledApp};
use crate::updates;

/// Build the listing of installed apps.
///
/// Update checks reach out to registries, so they only run when asked: an
/// offline machine should still get an instant answer.
pub async fn list(docker: &Docker, check_updates: bool) -> Result<String> {
    let apps = app::list(docker).await?;
    if apps.is_empty() {
        return Ok("No apps installed. Install one with: erst install <app>".to_string());
    }
    let apps = sort_by_running(apps);

    // Ask every registry in parallel so a slow one does not add up.
    let checks: Vec<Option<bool>> = if check_updates {
        join_all(
            apps.iter()
                .map(|app| updates::available(docker, &app.settings.image)),
        )
        .await
    } else {
        Vec::new()
    };

    let mut out = format!(
        "{:<20} {:<30} {:<14} {:<14} {:<10} {:<10}\n",
        "NAME", "IMAGE", "STATUS", "PORTS", "USER", "UPDATE"
    );
    for (index, app) in apps.iter().enumerate() {
        let update = match checks.get(index) {
            Some(check) => update_column(*check),
            None => "-",
        };
        out.push_str(&format!(
            "{:<20} {:<30} {:<14} {:<14} {:<10} {:<10}\n",
            app.settings.name,
            app.settings.image,
            app.status,
            ports(app),
            user_column(app),
            update
        ));
    }

    if checks.iter().any(|check| matches!(check, Some(true))) {
        out.push_str("\nSome apps have a newer image: erst update <app>");
    }
    Ok(out)
}

fn ports(app: &InstalledApp) -> String {
    app.settings
        .ports
        .iter()
        .map(|port| port.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The container's user, flagged with `!` when it is root.
fn user_column(app: &InstalledApp) -> String {
    if app.settings.runs_as_root() {
        "root!".to_string()
    } else {
        app.settings.user.trim().to_string()
    }
}

/// Whether a newer image exists, when the registry could be reached.
fn update_column(update: Option<bool>) -> &'static str {
    match update {
        Some(true) => "available",
        Some(false) => "current",
        None => "?",
    }
}

fn sort_by_running(mut apps: Vec<InstalledApp>) -> Vec<InstalledApp> {
    apps.sort_by(|a, b| {
        b.running
            .cmp(&a.running)
            .then_with(|| a.settings.name.cmp(&b.settings.name))
    });
    apps
}
