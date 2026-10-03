use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use bollard::Docker;
use bollard::models::{ContainerCreateBody, ContainerSummary, ContainerSummaryStateEnum};
use bollard::query_parameters::{ListContainersOptionsBuilder, RemoveContainerOptionsBuilder};
use serde::{Deserialize, Serialize};

/// Label that marks a container as managed by erst and carries its settings.
pub const LABEL: &str = "erst";

/// Settings of an installed app, stored as JSON in the container label.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSettings {
    pub name: String,
    pub image: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub ports: Vec<PortMapping>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Effective user of the container, as declared by the image.
    /// Empty means the image runs as root.
    #[serde(default)]
    pub user: String,
    /// Whether hardening (`no-new-privileges` + reduced capabilities) applies.
    #[serde(default = "default_true")]
    pub harden: bool,
    /// Paths inside the container that hold the app's data, one named volume
    /// per path. Empty for images that declare no data.
    #[serde(default)]
    pub data_paths: Vec<String>,
    /// Memory cap in bytes; `None` lets the app use as much as it wants.
    #[serde(default)]
    pub memory: Option<i64>,
    /// CPU allowance in nano-CPUs (`1_000_000_000` is one whole CPU).
    #[serde(default)]
    pub cpu_nanos: Option<i64>,
}

fn default_true() -> bool {
    true
}

impl AppSettings {
    /// Settings for a fresh install; the user and hardening flags are filled
    /// in once the image has been pulled and inspected.
    pub fn new(name: impl Into<String>, image: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            image: image.into(),
            host: None,
            ports: Vec::new(),
            env: BTreeMap::new(),
            user: String::new(),
            harden: true,
            data_paths: Vec::new(),
            memory: None,
            cpu_nanos: None,
        }
    }

    /// Access URLs derived from the published ports.
    pub fn urls(&self) -> Vec<String> {
        self.ports
            .iter()
            .map(|port| format!("http://localhost:{}", port.host))
            .collect()
    }

    /// Whether the container runs as root inside the container.
    pub fn runs_as_root(&self) -> bool {
        let user = self.user.trim();
        user.is_empty() || user == "root" || user == "0" || user == "0:0"
    }

    /// How the resource limits read, e.g. `512m · 1.5cpu`.
    pub fn limits(&self) -> String {
        crate::limits::show(self.memory, self.cpu_nanos)
    }
}

/// A port published by the app: host port -> container port.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortMapping {
    pub host: u16,
    pub container: u16,
}

impl std::str::FromStr for PortMapping {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        let parts: Vec<&str> = value.split(':').collect();
        match parts.as_slice() {
            [container] => {
                let container = parse_port(container)?;
                Ok(PortMapping {
                    host: container,
                    container,
                })
            }
            [host, container] => Ok(PortMapping {
                host: parse_port(host)?,
                container: parse_port(container)?,
            }),
            _ => bail!("invalid port `{value}`, expected HOST:CONTAINER or CONTAINER"),
        }
    }
}

impl std::fmt::Display for PortMapping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.host, self.container)
    }
}

fn parse_port(value: &str) -> Result<u16> {
    value
        .parse()
        .with_context(|| format!("invalid port `{value}`"))
}

/// An app as it currently exists in the runtime.
#[derive(Debug, Clone)]
pub struct InstalledApp {
    pub settings: AppSettings,
    pub container_id: String,
    pub status: String,
    pub running: bool,
}

pub fn container_name(name: &str) -> String {
    format!("erst-app-{name}")
}

/// List every app managed by erst, including stopped ones.
pub async fn list(docker: &Docker) -> Result<Vec<InstalledApp>> {
    // Filter by presence of the `erst` label.
    let filters = std::collections::HashMap::from([("label".to_string(), vec![LABEL.to_string()])]);
    let options = ListContainersOptionsBuilder::new()
        .all(true)
        .filters(&filters)
        .build();

    let containers = docker
        .list_containers(Some(options))
        .await
        .context("failed to list containers")?;

    Ok(containers.into_iter().filter_map(parse_container).collect())
}

/// Find an app by name.
pub async fn find(docker: &Docker, name: &str) -> Result<InstalledApp> {
    list(docker)
        .await?
        .into_iter()
        .find(|app| app.settings.name == name)
        .with_context(|| format!("app `{name}` is not installed"))
}

fn parse_container(container: ContainerSummary) -> Option<InstalledApp> {
    let raw = container.labels?.get(LABEL)?.clone();
    let settings: AppSettings = serde_json::from_str(&raw).ok()?;
    Some(InstalledApp {
        settings,
        container_id: container.id?,
        status: container.status.unwrap_or_default(),
        running: container
            .state
            .as_ref()
            .is_some_and(|state| matches!(state, ContainerSummaryStateEnum::RUNNING)),
    })
}

/// Remove an app container, optionally dropping its data volumes.
pub async fn remove(docker: &Docker, app: &InstalledApp, remove_data: bool) -> Result<()> {
    let options = RemoveContainerOptionsBuilder::new()
        .force(true)
        .v(remove_data)
        .build();
    docker
        .remove_container(&app.container_id, Some(options))
        .await
        .with_context(|| format!("failed to remove app `{}`", app.settings.name))?;
    Ok(())
}

/// Build the container body for an app's settings.
pub fn container_body(settings: &AppSettings) -> ContainerCreateBody {
    let mut labels = std::collections::HashMap::new();
    labels.insert(
        LABEL.to_string(),
        serde_json::to_string(settings).expect("settings are always serializable"),
    );

    let env: Vec<String> = settings
        .env
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();

    let exposed_ports: Vec<String> = settings
        .ports
        .iter()
        .map(|port| format!("{}/tcp", port.container))
        .collect();

    let mut port_bindings = std::collections::HashMap::new();
    for port in &settings.ports {
        port_bindings.insert(
            format!("{}/tcp", port.container),
            Some(vec![bollard::models::PortBinding {
                host_ip: None,
                host_port: Some(port.host.to_string()),
            }]),
        );
    }

    let security_opt = if settings.harden {
        Some(vec!["no-new-privileges:true".to_string()])
    } else {
        None
    };

    // Capabilities that ordinary apps do not need and that widen the blast
    // radius of a container escape. Apps that need them can opt out with
    // `erst install --no-harden`.
    let cap_drop = if settings.harden {
        Some(vec![
            "AUDIT_WRITE".to_string(),
            "MKNOD".to_string(),
            "SETFCAP".to_string(),
        ])
    } else {
        None
    };

    ContainerCreateBody {
        image: Some(settings.image.clone()),
        labels: Some(labels),
        env: Some(env),
        exposed_ports: Some(exposed_ports),
        host_config: Some(bollard::models::HostConfig {
            port_bindings: Some(port_bindings),
            restart_policy: Some(bollard::models::RestartPolicy {
                name: Some(bollard::models::RestartPolicyNameEnum::UNLESS_STOPPED),
                maximum_retry_count: None,
            }),
            security_opt,
            cap_drop,
            memory: settings.memory,
            nano_cpus: settings.cpu_nanos,
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Labels outlive the binary that wrote them, so settings saved by an
    /// earlier version have to keep reading after a new field is added.
    #[test]
    fn a_label_written_before_limits_existed_still_reads() {
        let label = r#"{"name":"nginx","image":"nginx:alpine","ports":[],"env":{},"user":"","harden":true,"data_paths":[]}"#;
        let settings: AppSettings = serde_json::from_str(label).expect("older labels still parse");

        assert_eq!(settings.memory, None);
        assert_eq!(settings.cpu_nanos, None);
        assert_eq!(settings.limits(), "-");
    }

    #[test]
    fn limits_reach_the_container() {
        let mut settings = AppSettings::new("redis", "redis:alpine");
        settings.memory = Some(256 * 1024 * 1024);
        settings.cpu_nanos = Some(500_000_000);

        let host = container_body(&settings)
            .host_config
            .expect("a host config is always set");

        assert_eq!(host.memory, Some(268_435_456));
        assert_eq!(host.nano_cpus, Some(500_000_000));
    }

    #[test]
    fn an_app_without_limits_is_left_unlimited() {
        let settings = AppSettings::new("nginx", "nginx:alpine");

        let host = container_body(&settings)
            .host_config
            .expect("a host config is always set");

        assert_eq!(host.memory, None);
        assert_eq!(host.nano_cpus, None);
    }
}
