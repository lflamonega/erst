mod app;
mod catalog;
mod cli;
mod commands;
mod runtime;

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
        Command::List => commands::list(&docker).await,
        Command::Logs { app, follow, tail } => commands::logs(&docker, &app, follow, tail).await,
        Command::Start { app } => commands::start(&docker, &app).await,
        Command::Stop { app } => commands::stop(&docker, &app).await,
        Command::Restart { app } => commands::restart(&docker, &app).await,
        Command::Update { app } => commands::update(&docker, &app).await,
        Command::Remove { app, remove_data } => commands::remove(&docker, &app, remove_data).await,
        Command::Catalog => unreachable!("catalog is handled before connecting"),
    }
}
