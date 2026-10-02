use anyhow::Result;

use crate::catalog;

/// List the apps available in the built-in catalog.
pub fn catalog() -> Result<String> {
    let entries = catalog::entries();
    if entries.is_empty() {
        return Ok("The catalog is empty.".to_string());
    }

    let mut out = format!("{:<20} {:<30} DESCRIPTION\n", "NAME", "IMAGE");
    for entry in entries {
        out.push_str(&format!(
            "{:<20} {:<30} {}\n",
            entry.name, entry.image, entry.description
        ));
    }
    out.push_str("\nInstall one with: erst install <name>");
    Ok(out)
}
