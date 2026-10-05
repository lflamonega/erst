//! Networks that let apps reach each other by name.
//!
//! Two apps installed separately live on the runtime's default network, where
//! they can only meet through published ports on the host. Putting them on a
//! shared network gives them DNS: `erst install mysql --network blog` and
//! `erst install wordpress --network blog` then talk over `mysql:3406` with
//! nothing published beyond what each declares.

use std::collections::HashMap;

use anyhow::{Context, Result};
use bollard::Docker;
use bollard::models::{NetworkCreateRequest, NetworkInspect};

use crate::app;

/// Label marking a network as created, and therefore removable, by erst.
pub const LABEL: &str = "erst";

/// The label value carried by managed networks.
const MANAGED: &str = "network";

/// The runtime's name for an erst network: `blog` becomes `erst-net-blog`, in
/// the same shape as `erst-app-<name>` and `erst-data-<name>-<n>`.
///
/// The prefix is what keeps a name read back from a container label from ever
/// matching a network that erst did not create.
pub fn full_name(name: &str) -> String {
    format!("erst-net-{}", app::safe_name(name))
}

/// Whether the network is there, ours or not.
///
/// The stack commands need to know after the fact whether the network their
/// apps shared is still around, which is a question about the runtime rather
/// than about ownership.
pub async fn exists(docker: &Docker, name: &str) -> bool {
    inspect(docker, &full_name(name)).await.is_some()
}

/// Create the network if it is not there yet, and return its runtime name.
pub async fn ensure(docker: &Docker, name: &str) -> Result<String> {
    let full = full_name(name);
    if inspect(docker, &full).await.is_some() {
        return Ok(full);
    }

    let mut labels = HashMap::new();
    labels.insert(LABEL.to_string(), MANAGED.to_string());
    let request = NetworkCreateRequest {
        name: full.clone(),
        driver: Some("bridge".to_string()),
        labels: Some(labels),
        ..Default::default()
    };

    if let Err(error) = docker.create_network(request).await {
        // Two apps of the same stack installed at once: the other one may have
        // won the race, which is the network we wanted anyway.
        if inspect(docker, &full).await.is_some() {
            return Ok(full);
        }
        return Err(error).with_context(|| format!("could not create network `{full}`"));
    }

    Ok(full)
}

/// Remove the network once nothing is attached to it.
///
/// Empty is the condition, not "the app that named it": the same network can
/// carry several apps, and the one that happens to be removed last is no more
/// entitled to it than the first.
pub async fn remove_if_empty(docker: &Docker, name: &str) -> Result<bool> {
    let full = full_name(name);
    let Some(network) = inspect(docker, &full).await else {
        return Ok(false);
    };

    // Only networks with our label: a name that merely looks like ours is not
    // ours to delete, whoever wrote it into a container label.
    if network
        .labels
        .as_ref()
        .and_then(|labels| labels.get(LABEL))
        .map(String::as_str)
        != Some(MANAGED)
    {
        return Ok(false);
    }

    if !network.containers.unwrap_or_default().is_empty() {
        return Ok(false);
    }

    docker
        .remove_network(&full)
        .await
        .with_context(|| format!("could not remove network `{full}`"))?;
    Ok(true)
}

/// The network as the runtime knows it, or `None` when there is no such one.
async fn inspect(docker: &Docker, name: &str) -> Option<NetworkInspect> {
    docker.inspect_network(name, None).await.ok()
}

#[cfg(test)]
mod tests {
    use super::full_name;

    #[test]
    fn a_network_name_gets_the_erst_prefix() {
        assert_eq!(full_name("blog"), "erst-net-blog");
        assert_eq!(full_name("My Net"), "erst-net-my-net");
        // The prefix is the whole point: a name written into a container label
        // can never come out the other side as a network Docker already had.
        assert_eq!(full_name("bridge"), "erst-net-bridge");
        let traversal = full_name("../../../etc");
        assert!(traversal.starts_with("erst-net-"), "{traversal}");
        assert!(
            !traversal.contains('/') && !traversal.contains('.'),
            "{traversal}"
        );
    }

    #[test]
    fn a_name_that_means_nothing_still_has_one() {
        assert_eq!(full_name(""), "erst-net-app");
        assert_eq!(full_name("!!!"), "erst-net-app");
    }
}
