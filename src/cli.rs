use clap::{Parser, Subcommand};

/// Install and manage containers without knowing about containers.
#[derive(Parser, Debug)]
#[command(name = "erst", version, about, max_term_width = 100)]
pub struct Cli {
    /// The dashboard opens when no subcommand is given.
    #[command(subcommand)]
    pub command: Option<Command>,
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
        /// Install without hardening (no-new-privileges and reduced
        /// capabilities). Needed by apps that escalate privileges on start.
        #[arg(long)]
        no_harden: bool,
        /// Cap memory: 512m, 1g, a bare 512 (megabytes), or `unlimited`.
        #[arg(long, value_name = "SIZE")]
        memory: Option<String>,
        /// Cap CPUs: 1, 0.5, 2.5, or `unlimited`.
        #[arg(long, value_name = "CPUS")]
        cpu: Option<String>,
    },
    /// Change an app's resource limits.
    Limit {
        /// App name.
        app: String,
        /// Cap memory: 512m, 1g, a bare 512 (megabytes), or `unlimited`.
        #[arg(long, value_name = "SIZE")]
        memory: Option<String>,
        /// Cap CPUs: 1, 0.5, 2.5, or `unlimited`.
        #[arg(long, value_name = "CPUS")]
        cpu: Option<String>,
    },
    /// Start an app when you log in, without needing root.
    Enable { app: String },
    /// Stop starting an app when you log in.
    Disable { app: String },
    /// Open the dashboard.
    Dashboard,
    /// List installed apps.
    #[command(visible_alias = "ls")]
    List {
        /// Ask each registry whether a newer image exists.
        #[arg(long)]
        check_updates: bool,
    },
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
    /// Save an app's data to a tar.gz file.
    Backup {
        /// App name.
        app: String,
        /// Destination file. Defaults to <app>-<timestamp>.tar.gz.
        destination: Option<String>,
    },
    /// Recreate an app from a backup file.
    Restore {
        /// Backup file created by `erst backup`.
        backup: String,
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
    /// Search Docker Hub for images.
    Search {
        /// What to look for.
        query: String,
        /// How many results to show.
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: usize,
    },
}
