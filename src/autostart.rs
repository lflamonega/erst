//! Starting apps at login, without needing root.
//!
//! A container restarts on its own whenever its runtime comes back, because it
//! is created with `restart: unless-stopped`. What that does not cover is the
//! machine rebooting while the runtime either starts with the user session or
//! is daemonless (Podman), where nothing is left to do the restarting. This is
//! the missing piece: a service the operating system already knows to run when
//! you log in, written where it already looks rather than in a file of our own
//! — so `erst` needs no privileges to set it up or to take it away.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::app;

/// The service manager this machine can use for the current user, when it has
/// one. Linux needs systemd actually running: the `systemctl` binary also
/// ships in containers and WSL, where nothing would ever execute the unit.
fn manager() -> Option<Manager> {
    if cfg!(target_os = "macos") {
        return Some(Manager::Launchd);
    }
    if cfg!(target_os = "linux") && Path::new("/run/systemd/system").is_dir() {
        return Some(Manager::Systemd);
    }
    None
}

enum Manager {
    Systemd,
    Launchd,
}

/// Whether the app is set to start when you log in.
pub fn enabled(name: &str) -> bool {
    entry_path(name).is_some_and(|path| path.exists())
}

/// Set autostart for an installed app, on or off.
pub async fn set(name: &str, on: bool) -> Result<String> {
    let path = entry_path(name).ok_or_else(unavailable)?;

    if on {
        enable(name, &path).await?;
        Ok(format!("{name} will start when you log in"))
    } else {
        disable(&path).await;
        Ok(format!("{name} will no longer start when you log in"))
    }
}

/// Drop any autostart entry for an app.
///
/// Best effort: removing an app should not fail because the service manager is
/// unhappy, and a leftover entry pointing at a name that no longer exists would
/// be worse than an ignored error.
pub async fn remove(name: &str) {
    if let Some(path) = entry_path(name) {
        disable(&path).await;
    }
}

/// Where an app's autostart entry lives, when this machine has somewhere to
/// put one.
fn entry_path(name: &str) -> Option<PathBuf> {
    let name = app::safe_name(name);
    match manager()? {
        Manager::Systemd => Some(
            config_home()?
                .join("systemd/user")
                .join(format!("erst-{name}.service")),
        ),
        Manager::Launchd => Some(
            home()?
                .join("Library/LaunchAgents")
                .join(format!("com.erst.{name}.plist")),
        ),
    }
}

async fn enable(name: &str, path: &Path) -> Result<()> {
    let parent = path.parent().context("the service path has no directory")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("could not create `{}`", parent.display()))?;

    let executable = std::env::current_exe().context("could not find the erst executable")?;
    let executable = executable.to_string_lossy();

    let systemd = matches!(manager(), Some(Manager::Systemd));
    let contents = match manager().expect("entry_path found a manager") {
        Manager::Systemd => unit_file(&executable, name),
        Manager::Launchd => launch_agent(&executable, name),
    };
    std::fs::write(path, contents)
        .with_context(|| format!("could not write `{}`", path.display()))?;

    if systemd {
        // systemd keeps a symlink graph of what to start, so the unit has to be
        // registered. launchd reads the directory itself on the next login,
        // which is the only moment this entry matters anyway.
        if let Err(error) = register(path).await {
            // Leaving the file behind would make `erst list` claim an autostart
            // that is never going to happen.
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
    }

    Ok(())
}

async fn register(path: &Path) -> Result<()> {
    let unit = unit_name(path);
    run("systemctl", &["--user", "daemon-reload"]).await?;
    run("systemctl", &["--user", "enable", &unit]).await
}

/// Take an app's autostart entry away.
///
/// Best effort throughout: the file is what `erst` treats as the truth, so
/// nothing here may leave an entry registered that `list` would not show.
async fn disable(path: &Path) {
    if !path.exists() {
        return;
    }

    let systemd = matches!(manager(), Some(Manager::Systemd));
    if systemd {
        let _ = run("systemctl", &["--user", "disable", &unit_name(path)]).await;
    }

    let _ = std::fs::remove_file(path);

    if systemd {
        let _ = run("systemctl", &["--user", "daemon-reload"]).await;
    }
}

/// The unit name `systemctl` wants: the file name it was registered under.
fn unit_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

async fn run(program: &str, args: &[&str]) -> Result<()> {
    let output = tokio::process::Command::new(program)
        .args(args)
        .output()
        .await
        .with_context(|| format!("could not run `{program}`"))?;

    if output.status.success() {
        return Ok(());
    }

    let detail = String::from_utf8_lossy(&output.stderr);
    let detail = detail.trim();
    bail!(
        "`{program} {}` failed{}",
        args.join(" "),
        if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        }
    );
}

/// The systemd user unit that starts an app when you log in.
pub fn unit_file(executable: &str, app: &str) -> String {
    format!(
        "[Unit]\n\
         Description=erst: start the {app} app when you log in\n\
         # Let the runtime come up first. Ordering against a unit that does not\n\
         # exist on this machine does nothing at all.\n\
         After=docker.service docker.socket podman.socket\n\
         \n\
         [Service]\n\
         Type=oneshot\n\
         ExecStart=\"{executable}\" start {app}\n\
         RemainAfterExit=yes\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n"
    )
}

/// The macOS launch agent that starts an app when you log in.
pub fn launch_agent(executable: &str, app: &str) -> String {
    let label = format!("com.erst.{}", app::safe_name(app));
    let executable = xml_escape(executable);
    let app = xml_escape(app);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \x20<key>Label</key>\n\
         \x20<string>{label}</string>\n\
         \x20<key>ProgramArguments</key>\n\
         \x20<array>\n\
         \x20\x20<string>{executable}</string>\n\
         \x20\x20<string>start</string>\n\
         \x20\x20<string>{app}</string>\n\
         \x20</array>\n\
         \x20<key>RunAtLoad</key>\n\
         \x20<true/>\n\
         </dict>\n\
         </plist>\n"
    )
}

/// The reason autostart cannot be set up on this machine.
fn unavailable() -> anyhow::Error {
    anyhow::anyhow!(
        "autostart needs systemd on Linux or macOS; this machine has neither. \
         Apps restart while the runtime runs."
    )
}

/// The three characters that would otherwise end an XML text node early.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn home() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    if home.is_empty() {
        return None;
    }
    Some(home.into())
}

fn config_home() -> Option<PathBuf> {
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(value) if !value.is_empty() => Some(value.into()),
        _ => home().map(|home| home.join(".config")),
    }
}

#[cfg(test)]
mod tests {
    use super::{enabled, launch_agent, unit_file, xml_escape};

    #[test]
    fn the_unit_starts_the_app_through_erst() {
        let unit = unit_file("/usr/local/bin/erst", "postgres");

        assert!(unit.contains("ExecStart=\"/usr/local/bin/erst\" start postgres"));
        assert!(unit.contains("WantedBy=default.target"));
        // The runtime has to be up before the app can be started.
        assert!(unit.contains("After=docker.service"));
    }

    #[test]
    fn the_launch_agent_starts_the_app_through_erst() {
        let agent = launch_agent("/opt/homebrew/bin/erst", "uptime-kuma");

        assert!(agent.contains("<string>/opt/homebrew/bin/erst</string>"));
        assert!(agent.contains("<string>start</string>"));
        assert!(agent.contains("<string>uptime-kuma</string>"));
        assert!(agent.contains("<key>RunAtLoad</key>"));
    }

    #[test]
    fn paths_are_escaped_before_they_reach_xml() {
        let agent = launch_agent("/opt/a&b/erst", "redis");

        assert!(agent.contains("/opt/a&amp;b/erst"));
        assert!(!agent.contains("/opt/a&b/"));
    }

    #[test]
    fn an_app_nobody_asked_for_does_not_autostart() {
        assert!(!enabled("definitely-not-an-app"));
    }

    #[test]
    fn xml_escapes_the_three_characters_that_matter() {
        assert_eq!(xml_escape("a<b>&c"), "a&lt;b&gt;&amp;c");
    }
}
