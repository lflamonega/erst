use std::time::Duration;

use bollard::Docker;

/// How long to wait for a registry before giving up on the update check.
const TIMEOUT: Duration = Duration::from_secs(5);

/// Manifest media types a registry may answer with.
const MANIFEST_TYPES: &str = "application/vnd.oci.image.index.v1+json, \
     application/vnd.oci.image.manifest.v1+json, \
     application/vnd.docker.distribution.manifest.list.v2+json, \
     application/vnd.docker.distribution.manifest.v2+json";

/// Whether the registry has a newer image than the one installed locally.
///
/// Answers `None` when it cannot tell, so a slow, private or offline registry
/// never blocks `erst list`.
pub async fn available(docker: &Docker, image: &str) -> Option<bool> {
    let (registry, repository, reference) = parse(image)?;
    let local = local_digest(docker, image).await?;

    let client = reqwest::Client::builder().timeout(TIMEOUT).build().ok()?;
    let remote = manifest_digest(&client, &registry, &repository, &reference).await?;
    Some(needs_update(&local, &remote))
}

/// A different digest means the tag now points at a newer build.
fn needs_update(local: &str, remote: &str) -> bool {
    local != remote
}

/// Digest of the manifest a registry would serve for a reference.
async fn manifest_digest(
    client: &reqwest::Client,
    registry: &str,
    repository: &str,
    reference: &str,
) -> Option<String> {
    let url = format!("https://{registry}/v2/{repository}/manifests/{reference}");

    let response = client
        .head(&url)
        .header("Accept", MANIFEST_TYPES)
        .send()
        .await
        .ok()?;
    let response = if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        let challenge = response
            .headers()
            .get("www-authenticate")
            .and_then(|value| value.to_str().ok())?
            .to_string();
        let token = token(client, &challenge).await?;
        client
            .head(&url)
            .header("Accept", MANIFEST_TYPES)
            .bearer_auth(token)
            .send()
            .await
            .ok()?
    } else {
        response
    };

    if !response.status().is_success() {
        return None;
    }

    response
        .headers()
        .get("docker-content-digest")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

/// Exchange a `WWW-Authenticate: Bearer ...` challenge for an anonymous token.
///
/// Registries hand out read-only tokens this way; images that need credentials
/// are not the audience of `erst update`.
async fn token(client: &reqwest::Client, challenge: &str) -> Option<String> {
    let params = bearer_params(challenge)?;
    let realm = params.iter().find(|(k, _)| k == "realm")?.1.clone();

    let response = client.get(&realm).query(&params).send().await.ok()?;
    let body: serde_json::Value = response.json().await.ok()?;

    Some(
        body.get("token")
            .or_else(|| body.get("access_token"))?
            .as_str()?
            .to_string(),
    )
}

/// Parse `Bearer realm="...",service="...",scope="..."` into query parameters.
fn bearer_params(challenge: &str) -> Option<Vec<(String, String)>> {
    let (_, rest) = challenge.split_once(' ')?;

    let mut params = Vec::new();
    for part in split_commas(rest) {
        if let Some((key, value)) = part.split_once('=') {
            params.push((key.trim().to_string(), value.trim_matches('"').to_string()));
        }
    }

    (!params.is_empty()).then_some(params)
}

/// Split on commas that are not inside quotes, so a realm containing one stays
/// intact.
fn split_commas(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quoted = false;

    for (index, character) in value.char_indices() {
        match character {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                parts.push(value[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(value[start..].trim());

    parts.into_iter().filter(|part| !part.is_empty()).collect()
}

/// The digest of the image as stored locally, from its `RepoDigests`.
async fn local_digest(docker: &Docker, image: &str) -> Option<String> {
    let info = docker.inspect_image(image).await.ok()?;
    let digest = info.repo_digests?.into_iter().next()?;
    digest.split_once('@').map(|(_, digest)| digest.to_string())
}

/// Split an image reference into registry, repository and tag or digest.
///
/// Docker Hub is `registry-1.docker.io/library/nginx` behind the scenes, which
/// is the layout registries expect in a request path.
fn parse(image: &str) -> Option<(String, String, String)> {
    let (name, reference) = match image.split_once('@') {
        Some((name, digest)) => (name, digest.to_string()),
        None => match image.rsplit_once(':') {
            Some((name, tag)) if !tag.contains('/') => (name, tag.to_string()),
            _ => (image, "latest".to_string()),
        },
    };

    let (registry, repository) = match name.split_once('/') {
        // A first component with a dot or a port is a registry host.
        Some((first, rest)) if first.contains('.') || first.contains(':') => {
            (first.to_string(), rest.to_string())
        }
        // On Docker Hub `user/app` keeps its namespace, while a bare `app` is
        // an official image living under `library`.
        Some(_) => ("registry-1.docker.io".to_string(), name.to_string()),
        None => (
            "registry-1.docker.io".to_string(),
            format!("library/{name}"),
        ),
    };

    Some((registry, repository, reference))
}

#[cfg(test)]
mod tests {
    use super::{bearer_params, needs_update, parse};

    #[test]
    fn docker_hub_images_use_the_registry_api_layout() {
        let (registry, repository, reference) = parse("nginx").unwrap();
        assert_eq!(registry, "registry-1.docker.io");
        assert_eq!(repository, "library/nginx");
        assert_eq!(reference, "latest");
    }

    #[test]
    fn tags_and_digests_are_kept() {
        assert_eq!(
            parse("nginx:alpine").unwrap(),
            (
                "registry-1.docker.io".to_string(),
                "library/nginx".to_string(),
                "alpine".to_string()
            )
        );
        assert_eq!(parse("nginx@sha256:abc").unwrap().2, "sha256:abc");
    }

    #[test]
    fn other_registries_and_userspaces_are_respected() {
        assert_eq!(
            parse("ghcr.io/acme/app:1").unwrap(),
            (
                "ghcr.io".to_string(),
                "acme/app".to_string(),
                "1".to_string()
            )
        );
        assert_eq!(
            parse("louislam/uptime-kuma:1").unwrap(),
            (
                "registry-1.docker.io".to_string(),
                "louislam/uptime-kuma".to_string(),
                "1".to_string()
            )
        );
        // A registry with a port is not a namespace.
        assert_eq!(
            parse("registry.example.com:5000/team/app").unwrap().0,
            "registry.example.com:5000"
        );
    }

    #[test]
    fn a_different_remote_digest_means_an_update() {
        let local = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
        assert!(!needs_update(local, local));

        let newer = "sha256:2222222222222222222222222222222222222222222222222222222222222222";
        assert!(needs_update(local, newer));
    }

    #[test]
    fn bearer_challenges_become_query_parameters() {
        let params = bearer_params(
            r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:library/nginx:pull""#,
        )
        .unwrap();

        assert_eq!(
            params,
            vec![
                (
                    "realm".to_string(),
                    "https://auth.docker.io/token".to_string()
                ),
                ("service".to_string(), "registry.docker.io".to_string()),
                (
                    "scope".to_string(),
                    "repository:library/nginx:pull".to_string()
                ),
            ]
        );
    }
}
