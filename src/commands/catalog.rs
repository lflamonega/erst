use anyhow::Result;

use crate::catalog;

/// List the apps available in the built-in catalog.
pub fn catalog() -> Result<()> {
    let entries = catalog::entries();
    if entries.is_empty() {
        println!("The catalog is empty.");
        return Ok(());
    }

    println!("{:<20} {:<30} DESCRIPTION", "NAME", "IMAGE");
    for entry in entries {
        println!(
            "{:<20} {:<30} {}",
            entry.name, entry.image, entry.description
        );
    }
    println!("\nInstall one with: erst install <name>");
    Ok(())
}
