mod backup;
mod catalog;
mod install;
mod lifecycle;
mod limit;
mod list;
mod logs;
mod recreate;
mod remove;
mod search;
mod update;

pub use backup::{backup, restore};
pub use catalog::catalog;
pub use install::{InstallOptions, install, install_entry};
pub use lifecycle::{restart, start, stop};
pub use limit::limit;
pub use list::list;
pub use logs::{logs, tail_logs};
pub use remove::remove;
pub use search::search;
pub use update::update;

/// Where a command writes its progress and warnings.
///
/// The CLI prints these straight to the terminal, while the dashboard collects
/// them to show in its status line: a command that printed on its own would
/// corrupt the alternate screen.
pub type Reporter<'a> = &'a mut dyn FnMut(&str);
