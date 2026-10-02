use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::app::{AppSettings, PortMapping};

/// An entry of the built-in catalog: a friendly name for a well-known image.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: &'static str,
    pub description: &'static str,
    pub image: &'static str,
    pub ports: &'static [u16],
    pub env: &'static [(&'static str, &'static str)],
}

const CATALOG: &[Entry] = &[
    Entry {
        name: "nginx",
        description: "Web server and reverse proxy",
        image: "nginx:alpine",
        ports: &[80],
        env: &[],
    },
    Entry {
        name: "redis",
        description: "In-memory key-value store",
        image: "redis:alpine",
        ports: &[6379],
        env: &[],
    },
    Entry {
        name: "postgres",
        description: "PostgreSQL database",
        image: "postgres:17-alpine",
        ports: &[5432],
        env: &[("POSTGRES_PASSWORD", "change-me")],
    },
    Entry {
        name: "mariadb",
        description: "MariaDB database",
        image: "mariadb:lts",
        ports: &[3306],
        env: &[("MARIADB_ROOT_PASSWORD", "change-me")],
    },
    Entry {
        name: "minio",
        description: "S3-compatible object storage",
        image: "minio/minio",
        ports: &[9001],
        env: &[
            ("MINIO_ROOT_USER", "erst"),
            ("MINIO_ROOT_PASSWORD", "change-me"),
        ],
    },
    Entry {
        name: "vaultwarden",
        description: "Bitwarden-compatible password manager",
        image: "vaultwarden/server:alpine",
        ports: &[80],
        env: &[],
    },
    Entry {
        name: "uptime-kuma",
        description: "Uptime monitoring",
        image: "louislam/uptime-kuma:1",
        ports: &[3001],
        env: &[],
    },
];

/// Find a catalog entry by name.
pub fn find(name: &str) -> Option<&'static Entry> {
    CATALOG.iter().find(|entry| entry.name == name)
}

/// List every catalog entry.
pub fn entries() -> &'static [Entry] {
    CATALOG
}

impl Entry {
    /// Turn this entry into app settings, applying `--port` and `--env` overrides.
    pub fn settings(&self, ports: &[String], env: &[String]) -> Result<AppSettings> {
        let port_mappings: Vec<PortMapping> = if ports.is_empty() {
            self.ports
                .iter()
                .map(|port| PortMapping {
                    host: *port,
                    container: *port,
                })
                .collect()
        } else {
            parse_ports(ports)?
        };

        let mut env_vars: BTreeMap<String, String> = self
            .env
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        env_vars.extend(parse_env(env)?);

        Ok(AppSettings {
            ports: port_mappings,
            env: env_vars,
            ..AppSettings::new(self.name, self.image)
        })
    }
}

/// Build settings for an arbitrary image reference (not in the catalog).
pub fn image_settings(reference: &str, ports: &[String], env: &[String]) -> Result<AppSettings> {
    let name = image_name(reference);

    // Port-less installs are allowed: the container simply publishes nothing.
    let port_mappings = parse_ports(ports)?;

    let mut settings = AppSettings::new(name, reference);
    settings.ports = port_mappings;
    settings.env = parse_env(env)?;
    Ok(settings)
}

fn parse_ports(values: &[String]) -> Result<Vec<PortMapping>> {
    values.iter().map(|value| value.parse()).collect()
}

pub fn parse_env(values: &[String]) -> Result<BTreeMap<String, String>> {
    let mut env = BTreeMap::new();
    for value in values {
        let Some((key, val)) = value.split_once('=') else {
            bail!("invalid env `{value}`, expected KEY=VALUE");
        };
        if key.is_empty() {
            bail!("invalid env `{value}`: key is empty");
        }
        env.insert(key.to_string(), val.to_string());
    }
    Ok(env)
}

/// Derive a container name from an image reference: `org/app:tag` -> `app`.
pub fn image_name(reference: &str) -> String {
    let last = reference.rsplit('/').next().unwrap_or(reference);
    let last = last.split(':').next().unwrap_or(last);
    let last = last.split('@').next().unwrap_or(last);

    let cleaned: String = last
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();

    let cleaned = cleaned.trim_matches('-');
    if cleaned.is_empty() {
        "app".to_string()
    } else {
        cleaned.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_name_strips_registry_tag_and_path() {
        assert_eq!(image_name("nginx:alpine"), "nginx");
        assert_eq!(image_name("library/redis:7"), "redis");
        assert_eq!(image_name("ghcr.io/acme/my-app:latest"), "my-app");
        assert_eq!(image_name("registry.example.com:5000/team/APP"), "app");
    }

    #[test]
    fn env_requires_key_value() {
        assert!(parse_env(&["KEY=1".to_string()]).is_ok());
        assert!(parse_env(&["KEY".to_string()]).is_err());
        assert!(parse_env(&["=1".to_string()]).is_err());
    }

    #[test]
    fn catalog_entry_builds_default_ports() {
        let entry = find("nginx").expect("nginx is in the catalog");
        let settings = entry.settings(&[], &[]).expect("valid settings");
        assert_eq!(settings.name, "nginx");
        assert_eq!(settings.ports.len(), 1);
        assert_eq!(settings.ports[0].host, 80);
        assert_eq!(settings.ports[0].container, 80);
    }

    #[test]
    fn port_overrides_replace_catalog_defaults() {
        let entry = find("nginx").expect("nginx is in the catalog");
        let settings = entry
            .settings(&["8080:80".to_string()], &[])
            .expect("valid settings");
        assert_eq!(settings.ports[0].host, 8080);
        assert_eq!(settings.ports[0].container, 80);
    }
}
