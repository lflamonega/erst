use anyhow::{Context, Result};

/// How long to wait for Docker Hub before giving up.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// An image found on Docker Hub, in the terms a user cares about.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    /// The reference to install with, e.g. `redis` or `louislam/uptime-kuma`.
    pub name: String,
    pub description: String,
    pub stars: u64,
    pub pulls: u64,
    /// Whether Docker publishes it itself, which is the trust signal that
    /// matters to someone who cannot judge an image by its author.
    pub official: bool,
}

/// What a search returned.
#[derive(Debug)]
pub struct Results {
    /// How many images matched in total, far more than are listed.
    pub total: u64,
    pub images: Vec<Image>,
}

/// Search Docker Hub for images.
///
/// The search endpoint is reachable without credentials, so this works without
/// a `docker login`.
pub async fn search(query: &str, limit: usize) -> Result<Results> {
    let response = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .context("failed to prepare the search")?
        .get("https://hub.docker.com/v2/search/repositories/")
        .query(&[
            ("query", query),
            ("page_size", &limit.clamp(1, 100).to_string()),
        ])
        .send()
        .await
        .context("Docker Hub could not be reached")?;

    let response: SearchResponse = response
        .json()
        .await
        .context("Docker Hub answered with something unexpected")?;

    Ok(Results {
        total: response.count,
        images: response.results.into_iter().map(Into::into).collect(),
    })
}

/// Counts as people read them: 11351782842 becomes 11B.
pub fn humans(count: u64) -> String {
    match count {
        0..=999 => count.to_string(),
        1_000..=999_999 => format!("{}K", count / 1_000),
        1_000_000..=999_999_999 => format!("{}M", count / 1_000_000),
        _ => format!("{}B", count / 1_000_000_000),
    }
}

/// A short description, cut at a word boundary.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max).collect();
    // Back up to the last word boundary, but only when there is one: a single
    // long word is cut where it stands.
    if let Some(space) = cut.rfind(' ') {
        cut.truncate(space);
    }
    format!("{}…", cut.trim_end())
}

#[derive(serde::Deserialize)]
struct SearchResponse {
    #[serde(default)]
    count: u64,
    #[serde(default)]
    results: Vec<RawImage>,
}

#[derive(serde::Deserialize)]
struct RawImage {
    repo_name: String,
    #[serde(default)]
    repo_owner: String,
    #[serde(default)]
    short_description: String,
    #[serde(default)]
    star_count: u64,
    #[serde(default)]
    pull_count: u64,
    #[serde(default)]
    is_official: bool,
}

impl From<RawImage> for Image {
    fn from(raw: RawImage) -> Self {
        // `repo_owner` and `repo_name` disagree about where the namespace
        // lives, so trust whichever already carries it.
        let name = if raw.repo_owner.is_empty()
            || raw.repo_name.starts_with(&format!("{}/", raw.repo_owner))
        {
            raw.repo_name
        } else {
            format!("{}/{}", raw.repo_owner, raw.repo_name)
        };

        Self {
            name,
            description: raw.short_description,
            stars: raw.star_count,
            pulls: raw.pull_count,
            official: raw.is_official,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Image, RawImage, humans, truncate};

    fn raw(repo_name: &str, repo_owner: &str) -> RawImage {
        RawImage {
            repo_name: repo_name.to_string(),
            repo_owner: repo_owner.to_string(),
            short_description: String::new(),
            star_count: 1,
            pull_count: 2,
            is_official: false,
        }
    }

    #[test]
    fn counts_are_compact() {
        assert_eq!(humans(0), "0");
        assert_eq!(humans(999), "999");
        assert_eq!(humans(13_619), "13K");
        assert_eq!(humans(31_690_367), "31M");
        assert_eq!(humans(11_351_782_842), "11B");
    }

    #[test]
    fn the_installable_name_keeps_a_namespace_only_once() {
        assert_eq!(Image::from(raw("redis", "")).name, "redis");

        // The API sometimes carries the namespace in both fields.
        assert_eq!(
            Image::from(raw("louislam/uptime-kuma", "louislam")).name,
            "louislam/uptime-kuma"
        );

        assert_eq!(
            Image::from(raw("uptime-kuma", "louislam")).name,
            "louislam/uptime-kuma"
        );
    }

    #[test]
    fn descriptions_are_cut_at_a_word() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("a rather long description here", 12), "a rather…");
        assert_eq!(truncate("supercalifragilistic", 5), "super…");
    }
}
