use crate::cache::JsonCache;
use anyhow::Result;

/// Execute the clear_cache command
pub async fn execute() -> Result<()> {
    let cache = JsonCache::new();

    println!("Clearing cache...");

    cache.clear().await?;

    println!("✓ Cache cleared successfully!");

    Ok(())
}
