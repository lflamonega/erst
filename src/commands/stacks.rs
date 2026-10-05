use std::collections::HashSet;

use anyhow::{Result, bail};
use bollard::Docker;

use crate::app::{self, InstalledApp};
use crate::network;
use crate::stacks::{self, Stack};

use super::Reporter;
use super::install::install_settings;
// Under another name: this module's `remove` takes the whole stack, this one
// takes a single app.
use super::remove::remove as remove_app;

/// List every stack and how much of it is installed.
pub async fn list(docker: &Docker) -> Result<String> {
    // One listing for every stack: `app::find` per member would ask the
    // runtime the same question once per app.
    let installed: HashSet<String> = app::list(docker)
        .await?
        .iter()
        .map(|app| app.settings.name.clone())
        .collect();

    let mut out = format!(
        "{:<10} {:<34} {:<10} {}\n",
        "NAME", "APPS", "INSTALLED", "WHAT YOU GET"
    );
    for stack in stacks::entries() {
        let found = stack
            .members
            .iter()
            .filter(|member| installed.contains(member.name))
            .count();
        out.push_str(&format!(
            "{:<10} {:<34} {:<10} {}\n",
            stack.name,
            stack.member_names().join(", "),
            format!("{found} of {}", stack.members.len()),
            stack.description
        ));
    }
    out.push_str("\nInstall one with: erst stack install <name>");
    Ok(out)
}

/// Install a whole stack: every member, on the network they share.
pub async fn install(docker: &Docker, name: &str, report: Reporter<'_>) -> Result<String> {
    let stack = find(name)?;

    // Check every member before installing any of them. Half a stack is worse
    // than none: the app would be up and talking to a database that is still
    // whatever the user had instead.
    let mut taken = Vec::new();
    for member in stack.members {
        if app::find(docker, member.name).await.is_ok() {
            taken.push(member.name);
        }
    }
    if !taken.is_empty() {
        bail!(
            "{} already installed; remove {} first so it can join the {} stack",
            taken.join(", "),
            taken.join(", "),
            stack.name
        );
    }

    for member in stack.members {
        let settings = member.settings(stack)?;
        // Each member reports its own progress and URL as it goes; the summary
        // comes after, once there is one to give.
        let message = install_settings(docker, settings, report).await?;
        report(&message);
    }

    Ok(format!(
        "Installed the {} stack ({}) on network {}",
        stack.name,
        stack.member_names().join(", "),
        stack.name
    ))
}

/// Remove a stack's apps, keeping their data unless asked otherwise.
pub async fn remove(docker: &Docker, name: &str, remove_data: bool) -> Result<String> {
    let stack = find(name)?;
    let had_network = network::exists(docker, stack.name).await;

    // Reverse order: the app first, its dependency last, so nothing is left
    // pointing at something that has already gone.
    let mut removed: Vec<InstalledApp> = Vec::new();
    for member in stack.members.iter().rev() {
        match app::find(docker, member.name).await {
            Ok(app) => {
                remove_app(docker, &app.settings.name, remove_data).await?;
                removed.push(app);
            }
            // A stack the user only partly installed still removes as far as
            // it went, rather than refusing over what is not there.
            Err(_) => continue,
        }
    }

    if removed.is_empty() {
        bail!("the {name} stack is not installed");
    }

    let mut message = format!(
        "Removed the {name} stack ({})",
        removed
            .iter()
            .map(|app| app.settings.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    if remove_data {
        message.push_str(" and its data");
    } else if removed
        .iter()
        .any(|app| !app.settings.data_paths.is_empty())
    {
        message.push_str("\n  Data was kept. Pass --remove-data to delete it too.");
    }

    // The last member to go takes the network with it, and that is worth
    // saying once here rather than once per member.
    if had_network && !network::exists(docker, stack.name).await {
        message.push_str(&format!("\n  The network {name} was removed too."));
    }

    Ok(message)
}

/// Resolve a stack name, or say what the names are.
fn find(name: &str) -> Result<&'static Stack> {
    match stacks::find(name) {
        Some(stack) => Ok(stack),
        None => bail!(
            "no stack named `{name}`. The stacks are: {}",
            stacks::entries()
                .iter()
                .map(|stack| stack.name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}
