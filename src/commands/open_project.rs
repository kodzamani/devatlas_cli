use crate::cache::JsonCache;
use crate::editor::EditorDetector;
use crate::models::CodeEditor;
use anyhow::Result;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::io::{stdin, stdout, Write};

/// Execute the open_project command
pub async fn execute(
    name: Option<String>,
    path: Option<String>,
    editor: Option<String>,
    select: bool,
) -> Result<()> {
    let cache = JsonCache::new();
    let editor_detector = EditorDetector::new();

    // Load projects from cache or scan if needed
    let projects = if cache.needs_rescan().await {
        println!("No cached projects found. Scanning drives first...");
        let scanner = crate::scanner::ProjectScanner::new();
        let found = scanner.scan_all_drives(None).await?;
        cache.save_projects(found.clone()).await?;
        found
    } else {
        cache.load_projects().await?
    };

    if projects.is_empty() {
        println!("No projects found. Please run 'devatlas scan' first.");
        return Ok(());
    }

    // Determine which project to open
    let project_path = if let Some(p) = path {
        // Direct path provided
        if !std::path::Path::new(&p).exists() {
            anyhow::bail!("Path does not exist: {}", p);
        }
        p
    } else if let Some(n) = name {
        // Find project by name
        find_project_by_name(&projects, &n)?
    } else {
        // Interactive selection
        select_project_interactively(&projects)?
    };

    // Determine which editor to use
    let selected_editor = if let Some(ed) = editor {
        // Specific editor requested
        match editor_detector.get_editor_by_name(&ed) {
            Some(e) if e.is_installed => e,
            _ => {
                println!("Editor '{}' is not installed.", ed);
                println!("Showing available editors...\n");
                prompt_for_editor_selection(&editor_detector)?
            }
        }
    } else {
        if select {
            println!("Select flag detected. Choose an editor:\n");
        }
        prompt_for_editor_selection(&editor_detector)?
    };

    // Open the project in the editor
    println!(
        "Opening '{}' in {}...",
        project_path, selected_editor.display_name
    );

    EditorDetector::open_in_editor(&selected_editor, &project_path)?;

    println!("Done!");
    Ok(())
}

/// Find project by name (fuzzy match)
fn find_project_by_name(projects: &[crate::models::Project], name: &str) -> Result<String> {
    let matcher = SkimMatcherV2::default();
    
    // Try exact match first
    if let Some(project) = projects.iter().find(|p| p.name.to_lowercase() == name.to_lowercase()) {
        return Ok(project.path.clone());
    }

    // Try fuzzy match
    let mut matches: Vec<(i64, &crate::models::Project)> = projects
        .iter()
        .filter_map(|p| {
            matcher
                .fuzzy_match(&p.name, name)
                .map(|score| (score, p))
        })
        .collect();

    matches.sort_by(|a, b| b.0.cmp(&a.0));

    if let Some((_, project)) = matches.first() {
        Ok(project.path.clone())
    } else {
        anyhow::bail!("Project not found: {}", name)
    }
}

/// Select project interactively
fn select_project_interactively(projects: &[crate::models::Project]) -> Result<String> {
    if projects.is_empty() {
        anyhow::bail!("No projects available");
    }

    println!("\nSelect a project:");
    for (i, project) in projects.iter().enumerate().take(20) {
        println!("  {:2}. {} ({})", i + 1, project.name, project.project_type);
    }

    if projects.len() > 20 {
        println!("  ... and {} more", projects.len() - 20);
    }

    print!("\nEnter number (1-{}): ", projects.len().min(20));
    let _ = stdout().flush();

    let mut input = String::new();
    stdin().read_line(&mut input)?;

    if let Ok(num) = input.trim().parse::<usize>() {
        if num >= 1 && num <= projects.len() {
            return Ok(projects[num - 1].path.clone());
        }
    }

    anyhow::bail!("Invalid selection")
}

fn prompt_for_editor_selection(detector: &EditorDetector) -> Result<CodeEditor> {
    let editors = detector.detect_installed_editors();

    if editors.is_empty() {
        println!("No editors found!");
        println!("Please install one of: VS Code, Cursor, Windsurf, Visual Studio");
        anyhow::bail!("No editors available");
    }

    println!("Which IDE do you want to use?");
    for (i, editor) in editors.iter().enumerate() {
        println!("  {}. {}", i + 1, editor.display_name);
    }

    match EditorDetector::select_editor_interactively(&editors) {
        Some(editor) => Ok(editor),
        None => anyhow::bail!("Invalid editor selection"),
    }
}
