use clap::{Parser, Subcommand};

/// Install and manage containers without knowing about containers.
#[derive(Parser, Debug)]
#[command(name = "erst", version, about, max_term_width = 100)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Install an app from the catalog or any container image.
    Install {
        /// Catalog app name or container image reference.
        app: String,
        /// Publish a port: HOST:CONTAINER (repeatable).
        #[arg(long = "port", value_name = "HOST:CONTAINER")]
        ports: Vec<String>,
        /// Set an environment variable: KEY=VALUE (repeatable).
        #[arg(long = "env", value_name = "KEY=VALUE")]
        env: Vec<String>,
        /// Hostname to access the app.
        #[arg(long)]
        host: Option<String>,
    },
    /// List installed apps.
    #[command(visible_alias = "ls")]
    List,
    /// Show the logs of an app.
    Logs {
        /// App name.
        app: String,
        /// Follow the log output.
        #[arg(short = 'f', long)]
        follow: bool,
        /// Number of lines to show from the end.
        #[arg(long, default_value_t = 100)]
        tail: u64,
    },
    /// Start a stopped app.
    Start {
        /// App name.
        app: String,
    },
    /// Stop a running app.
    Stop {
        /// App name.
        app: String,
    },
    /// Restart an app.
    Restart {
        /// App name.
        app: String,
    },
    /// Update an app to the latest version of its image.
    Update { app: String },
    /// Remove an app.
    #[command(visible_alias = "rm")]
    Remove {
        /// App name.
        app: String,
        /// Also remove the app data.
        #[arg(long)]
        remove_data: bool,
    },
    /// List the apps available in the catalog.
    Catalog,
}
