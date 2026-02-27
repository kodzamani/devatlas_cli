use crate::cache::JsonCache;
use crate::editor::EditorDetector;
use anyhow::Result;
use chrono::Utc;

/// Execute the status command
pub async fn execute() -> Result<()> {
    let cache = JsonCache::new();
    let editor_detector = EditorDetector::new();

    println!("{}", "═".repeat(60));
    println!("DevAtlas CLI Status");
    println!("{}\n", "═".repeat(60));

    // Cache status
    println!("📁 Cache Status:");
    let project_count = cache.get_project_count().await;
    let last_indexed = cache.get_last_indexed().await;
    let cache_size = cache.get_cache_size();

    println!("  Projects cached: {}", project_count);
    if let Some(time) = last_indexed {
        let hours_ago = (Utc::now() - time).num_hours();
        println!("  Last indexed: {} hours ago", hours_ago);
    } else {
        println!("  Last indexed: Never");
    }
    println!("  Cache size: {} bytes", cache_size);
    println!("  Cache path: {:?}", cache.get_cache_path());

    // Check if rescan is needed
    let needs_rescan = cache.needs_rescan().await;
    if needs_rescan {
        println!("  ⚠️  Rescan needed: Cache is older than 24 hours");
    } else {
        println!("  ✓ Cache is up to date");
    }

    // Editor status
    println!("\n📝 Available Editors:");
    let editors = editor_detector.detect_installed_editors();
    let installed: Vec<_> = editors.iter().filter(|e| e.is_installed).collect();

    if installed.is_empty() {
        println!("  No editors found!");
    } else {
        for editor in installed {
            println!("  ✓ {} ({})", editor.display_name, editor.name);
            if let Some(path) = &editor.full_path {
                println!("      Path: {}", path);
            }
        }
    }

    let not_installed: Vec<_> = editors.iter().filter(|e| !e.is_installed).collect();
    if !not_installed.is_empty() {
        println!("\n❌ Not installed:");
        for editor in not_installed {
            println!("  - {} ({})", editor.display_name, editor.name);
        }
    }

    println!("\n{}", "═".repeat(60));

    Ok(())
}
