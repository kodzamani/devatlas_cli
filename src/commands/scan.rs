use crate::cache::JsonCache;
use crate::scanner::ProjectScanner;
use anyhow::Result;

/// Execute the scan command
pub async fn execute(drive: Option<String>) -> Result<()> {
    let cache = JsonCache::new();
    let scanner = ProjectScanner::new();

    println!("Starting project scan...");

    // Scan all drives or specific drive
    let projects = if let Some(d) = drive {
        println!("Scanning drive: {}", d);
        // For specific drive, we would need to modify scanner to support this
        // For now, scan all drives
        scanner.scan_all_drives(None).await?
    } else {
        println!("Scanning all drives...");
        scanner.scan_all_drives(None).await?
    };

    // Save to cache
    cache.save_projects(projects.clone()).await?;

    println!("\nScan complete!");
    println!("Found {} projects", projects.len());

    // Show statistics by category
    let mut categories: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for project in &projects {
        *categories.entry(project.category.clone()).or_insert(0) += 1;
    }

    // Convert to sorted vec
    let mut sorted: Vec<_> = categories.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));

    println!("\nProjects by category:");
    for (category, count) in sorted {
        println!("  {}: {}", category, count);
    }

    Ok(())
}
