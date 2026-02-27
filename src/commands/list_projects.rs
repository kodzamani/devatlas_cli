use crate::cache::JsonCache;
use crate::scanner::ProjectScanner;
use anyhow::Result;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use tracing::{debug, info};

/// Execute the list_projects command
pub async fn execute(
    category: Option<String>,
    search: Option<String>,
    active_only: bool,
    rescan: bool,
) -> Result<()> {
    let cache = JsonCache::new();

    // Check if we need to rescan
    let needs_rescan = cache.needs_rescan().await || rescan;

    let projects = if needs_rescan {
        info!("Scanning for projects...");
        println!("Scanning drives for projects...");
        
        let scanner = ProjectScanner::new();
        let found_projects = scanner.scan_all_drives(None).await?;
        
        // Save to cache
        cache.save_projects(found_projects.clone()).await?;
        
        println!("Found {} projects", found_projects.len());
        found_projects
    } else {
        // Load from cache
        debug!("Loading projects from cache");
        cache.load_projects().await?
    };

    // Filter projects
    let mut filtered = projects;

    // Filter by category
    if let Some(cat) = &category {
        let cat_lower = cat.to_lowercase();
        filtered.retain(|p| p.category.to_lowercase() == cat_lower);
    }

    // Filter by search query
    if let Some(query) = &search {
        let matcher = SkimMatcherV2::default();
        filtered.retain(|p| {
            matcher.fuzzy_match(&p.name, query).is_some() ||
            matcher.fuzzy_match(&p.path, query).is_some() ||
            matcher.fuzzy_match(&p.project_type, query).is_some()
        });
    }

    // Filter by active status
    if active_only {
        filtered.retain(|p| p.is_active);
    }

    // Display projects
    if filtered.is_empty() {
        println!("No projects found.");
        return Ok(());
    }

    // Group by category for display
    println!("\n{}", "═".repeat(80));
    println!("{} Projects Found", filtered.len());
    println!("{}\n", "═".repeat(80));

    // Print header
    println!("{:4} {:<30} {:<35} {:<15}", "#", "Name", "Path", "Type");
    println!("{}", "-".repeat(80));

    for (i, project) in filtered.iter().enumerate() {
        let name = if project.name.len() > 28 {
            format!("{}...", &project.name[..25])
        } else {
            project.name.clone()
        };

        let path = if project.path.len() > 33 {
            format!("{}...", &project.path[..30])
        } else {
            project.path.clone()
        };

        println!(
            "{:4} {:<30} {:<35} {:<15}",
            i + 1,
            name,
            path,
            project.project_type
        );
    }

    println!("{}", "═".repeat(80));
    println!();

    Ok(())
}
