use anyhow::Result;

use crate::catalog;
use crate::hub;

/// Search Docker Hub and describe the matches.
pub async fn search(query: &str, limit: usize) -> Result<String> {
    let results = hub::search(query, limit).await?;
    let images = results.images;

    if images.is_empty() {
        return Ok(format!("No images found for `{query}`."));
    }

    // The leading column marks Docker's own images: for someone who does not
    // know the ecosystem, an official image is the safe default to install.
    let mut out = format!(
        " {:<34} {:>7} {:>7}  DESCRIPTION\n",
        "NAME", "STARS", "PULLS"
    );
    for image in &images {
        out.push_str(&format!(
            "{}{:<34} {:>7} {:>7}  {}\n",
            if image.official { "*" } else { " " },
            image.name,
            hub::humans(image.stars),
            hub::humans(image.pulls),
            hub::truncate(&image.description, 60)
        ));
    }

    out.push_str(&format!(
        "\n{} of about {} found. Install one with: erst install <name>",
        images.len(),
        hub::humans(results.total)
    ));
    if let Some(hint) = catalog_hint(query, &images) {
        out.push_str(&format!("\n{hint}"));
    }
    Ok(out)
}

/// Point at the catalog when the query or one of the results is in it, since
/// that path installs sensible defaults instead of asking for flags.
fn catalog_hint(query: &str, images: &[hub::Image]) -> Option<String> {
    let entry = images
        .iter()
        .find_map(|image| catalog::find_by_reference(&image.name))
        .or_else(|| catalog::find_by_reference(query))?;

    Some(format!(
        "`{}` is in the built-in catalog: erst install {}",
        entry.name, entry.name
    ))
}
