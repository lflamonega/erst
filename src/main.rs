mod app;
mod catalog;
mod cli;
mod commands;
mod data;
mod hub;
mod limits;
mod runtime;
mod tui;
mod updates;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // The catalog listing works without a container runtime, and the dashboard
    // owns the terminal while it runs.
    match &cli.command {
        None | Some(Command::Dashboard) => return tui::run(runtime::connect().await?).await,
        Some(Command::Catalog) => return print(commands::catalog()),
        Some(Command::Search { query, limit }) => {
            return print(commands::search(query, *limit).await);
        }
        _ => {}
    }

    let docker = runtime::connect().await?;

    match cli.command.expect("handled above") {
        Command::Install {
            app,
            ports,
            env,
            host,
            no_harden,
            memory,
            cpu,
        } => {
            let options = commands::InstallOptions {
                reference: app,
                ports,
                env,
                host,
                harden: !no_harden,
                memory,
                cpu,
            };
            print(commands::install(&docker, options, &mut echo).await)
        }
        Command::Limit { app, memory, cpu } => {
            print(commands::limit(&docker, &app, memory.as_deref(), cpu.as_deref()).await)
        }
        Command::List { check_updates } => print(commands::list(&docker, check_updates).await),
        Command::Logs { app, follow, tail } => commands::logs(&docker, &app, follow, tail).await,
        Command::Start { app } => print(commands::start(&docker, &app).await),
        Command::Stop { app } => print(commands::stop(&docker, &app).await),
        Command::Restart { app } => print(commands::restart(&docker, &app).await),
        Command::Backup { app, destination } => {
            let destination = destination.map_or_else(|| default_backup_name(&app), Into::into);
            print(commands::backup(&docker, &app, &destination, &mut echo).await)
        }
        Command::Restore { backup } => {
            print(commands::restore(&docker, std::path::Path::new(&backup)).await)
        }
        Command::Update { app } => print(commands::update(&docker, &app, &mut echo).await),
        Command::Remove { app, remove_data } => {
            print(commands::remove(&docker, &app, remove_data).await)
        }
        Command::Catalog | Command::Dashboard | Command::Search { .. } => {
            unreachable!("handled before connecting")
        }
    }
}

/// Print what a command returned, unless it only had something to say.
fn print(message: Result<String>) -> Result<()> {
    println!("{}", message?.trim_end_matches('\n'));
    Ok(())
}

/// Progress reporter for the plain CLI: straight to the terminal.
fn echo(line: &str) {
    println!("{line}");
}

/// Backup file name when the user does not give one: `<app>-<timestamp>.tar.gz`.
fn default_backup_name(app: &str) -> std::path::PathBuf {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{app}-{seconds}.tar.gz").into()
}
