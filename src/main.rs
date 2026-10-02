mod app;
mod catalog;
mod cli;
mod commands;
mod data;
mod runtime;
mod updates;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Only the catalog listing works without a container runtime.
    if let Command::Catalog = cli.command {
        return commands::catalog();
    }

    let docker = runtime::connect().await?;

    match cli.command {
        Command::Install {
            app,
            ports,
            env,
            host,
            no_harden,
        } => commands::install(&docker, &app, &ports, &env, host, no_harden).await,
        Command::List { check_updates } => commands::list(&docker, check_updates).await,
        Command::Logs { app, follow, tail } => commands::logs(&docker, &app, follow, tail).await,
        Command::Start { app } => commands::start(&docker, &app).await,
        Command::Stop { app } => commands::stop(&docker, &app).await,
        Command::Restart { app } => commands::restart(&docker, &app).await,
        Command::Backup { app, destination } => {
            let destination = destination.map_or_else(|| default_backup_name(&app), Into::into);
            commands::backup(&docker, &app, &destination).await
        }
        Command::Restore { backup } => {
            commands::restore(&docker, std::path::Path::new(&backup)).await
        }
        Command::Update { app } => commands::update(&docker, &app).await,
        Command::Remove { app, remove_data } => commands::remove(&docker, &app, remove_data).await,
        Command::Catalog => unreachable!("catalog is handled before connecting"),
    }
}

/// Backup file name when the user does not give one: `<app>-<timestamp>.tar.gz`.
fn default_backup_name(app: &str) -> std::path::PathBuf {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{app}-{seconds}.tar.gz").into()
}
