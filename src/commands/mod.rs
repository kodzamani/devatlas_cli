use crate::analyzer::ProjectAnalyzer;
use crate::cache::JsonCache;
use crate::dependencies::{DependencyDetector, PackageUpdateChecker};
use crate::editor::EditorDetector;
use crate::git_stats::GitStatsService;
use crate::models::{DateRangeFilter, ProjectInfo, ProjectMetric, StatsSummary};
use crate::runner::ProjectRunner;
use crate::scanner::ProjectScanner;
use crate::settings::SettingsStore;
use crate::unused::UnusedCodeAnalyzer;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::BTreeMap;
use std::io::{stdin, stdout, Write};
use std::path::Path;

pub async fn scan(path: Option<String>, drive: Option<String>) -> Result<()> {
    let settings = SettingsStore::new().load()?;
    let scanner = ProjectScanner::new(settings.exclude_paths);
    let cache = JsonCache::new();
    let target = path.or(drive);

    println!("Starting project scan...");
    let projects = match target {
        Some(target) => {
            println!("Scanning {target}...");
            scanner.scan_path(&target, None).await?
        }
        None => {
            println!("Scanning all drives...");
            scanner.scan_all_drives(None).await?
        }
    };

    cache.save_projects(projects.clone()).await?;
    print_project_inventory(&projects);
    Ok(())
}

pub async fn list_projects(
    category: Option<String>,
    search: Option<String>,
    active_only: bool,
    rescan: bool,
) -> Result<()> {
    let projects = ensure_projects_available(rescan).await?;
    let matcher = SkimMatcherV2::default();
    let mut filtered = projects
        .into_iter()
        .filter(|project| {
            category
                .as_ref()
                .map(|requested| project.category.eq_ignore_ascii_case(requested))
                .unwrap_or(true)
        })
        .filter(|project| !active_only || project.is_active)
        .filter_map(|project| {
            let search_rank = search
                .as_ref()
                .and_then(|query| rank_project_search_match(&matcher, &project, query));

            if search.is_some() && search_rank.is_none() {
                return None;
            }

            Some((project, search_rank))
        })
        .collect::<Vec<_>>();

    filtered.sort_by(|(left, left_rank), (right, right_rank)| {
        right_rank
            .cmp(left_rank)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    let filtered = filtered
        .into_iter()
        .map(|(project, _)| project)
        .collect::<Vec<_>>();

    if filtered.is_empty() {
        println!("No projects found.");
        return Ok(());
    }

    print_projects_table(&filtered);
    println!("Total: {}", filtered.len());
    Ok(())
}

pub async fn open_project(
    name: Option<String>,
    path: Option<String>,
    editor: Option<String>,
    _select: bool,
    rescan: bool,
) -> Result<()> {
    let project = resolve_project(name, path, rescan).await?;
    let detector = EditorDetector::new();
    let chosen_editor = if let Some(editor_name) = editor {
        detector
            .get_editor_by_name(&editor_name)
            .filter(|editor| editor.is_installed)
            .ok_or_else(|| anyhow::anyhow!("Editor not installed: {editor_name}"))?
    } else {
        prompt_for_editor(&detector)?
    };

    EditorDetector::open_in_editor(&chosen_editor, &project.path)?;
    println!("Opened {} in {}", project.name, chosen_editor.display_name);
    Ok(())
}

pub async fn run_project(
    name: Option<String>,
    path: Option<String>,
    script: Option<String>,
    detached: bool,
    install: bool,
    open_browser: bool,
    rescan: bool,
) -> Result<()> {
    let project = resolve_project(name, path, rescan).await?;
    let command = match script {
        Some(script) => script,
        None => ProjectRunner::get_start_command(&project.path)?
            .ok_or_else(|| anyhow::anyhow!("No runnable script found in package.json"))?,
    };

    if install && !ProjectRunner::has_node_modules(&project.path) {
        println!("Running npm install...");
        ProjectRunner::install(&project.path)?;
    }

    if open_browser {
        let url = "http://localhost:3000";
        let _ = ProjectRunner::open_browser(url);
        println!("Opening {url}");
    }

    println!("Running `{}` in {}", command, project.name);
    ProjectRunner::run(&project.path, &command, detached)
}

pub async fn analyze_project(
    name: Option<String>,
    path: Option<String>,
    show_files: bool,
    show_tech_stack: bool,
    rescan: bool,
) -> Result<()> {
    let project = resolve_project(name, path, rescan).await?;
    let analyzer = ProjectAnalyzer;
    let analysis = analyzer.analyze_project(&project.path);
    JsonCache::new()
        .save_single_project_metrics(&project.path, analysis.total_files, analysis.total_lines)
        .await?;

    print_kv_table(
        "Project Summary",
        &[
            ("Project", project.name.clone()),
            ("Path", project.path.clone()),
            (
                "Type",
                format!("{} / {}", project.category, project.project_type),
            ),
            ("Files", analysis.total_files.to_string()),
            ("Lines", analysis.total_lines.to_string()),
            ("Avg lines/file", analysis.avg_lines_per_file().to_string()),
            (
                "Largest file",
                format!(
                    "{} ({})",
                    analysis.largest_file_name, analysis.largest_file_lines
                ),
            ),
        ],
    );

    let language_rows = analysis
        .languages
        .iter()
        .take(12)
        .map(|language| {
            vec![
                language.name.clone(),
                format!("{:.1}", language.percentage),
                language.total_lines.to_string(),
                language.file_count.to_string(),
            ]
        })
        .collect::<Vec<_>>();
    print_table(
        "Languages",
        &["Language", "Percent", "Lines", "Files"],
        &language_rows,
    );

    if show_tech_stack {
        let tech_rows = analyzer
            .tech_stack_with_lines(&project.path, &project.tags)
            .into_iter()
            .map(|item| vec![item.name, item.lines.to_string()])
            .collect::<Vec<_>>();
        print_table("Tech Stack", &["Technology", "Lines"], &tech_rows);
    }

    if show_files {
        let file_rows = analysis
            .files
            .iter()
            .take(20)
            .map(|file| vec![file.lines.to_string(), file.relative_path.clone()])
            .collect::<Vec<_>>();
        print_table("Top Files", &["Lines", "Relative Path"], &file_rows);
    }

    Ok(())
}

pub async fn dependencies(
    name: Option<String>,
    path: Option<String>,
    check_updates: bool,
    rescan: bool,
) -> Result<()> {
    let project = resolve_project(name, path, rescan).await?;
    let detector = DependencyDetector;
    let mut sections = detector.detect(&project.path)?;

    if sections.is_empty() {
        println!("No dependency manifests found.");
        return Ok(());
    }

    if check_updates {
        println!("Checking package registries...");
        PackageUpdateChecker::new()
            .check_sections(&mut sections)
            .await;
    }

    for section in sections {
        let title = format!("{} {}", section.icon, section.name);
        for group in section.groups {
            let rows = group
                .packages
                .into_iter()
                .filter_map(|package| {
                    let latest = package.latest_version.unwrap_or_else(|| "-".to_string());
                    if latest != "-" && versions_match(&package.version, &latest) {
                        return None;
                    }

                    Some(vec![package.name, package.version, latest])
                })
                .collect::<Vec<_>>();
            if !rows.is_empty() {
                print_table(
                    &format!("{title} / {}", group.name),
                    &["Package", "Current", "Latest"],
                    &rows,
                );
            }
        }
    }

    Ok(())
}

pub async fn stats(
    range: DateRangeFilter,
    project_filter: Option<String>,
    top: usize,
    rescan: bool,
) -> Result<()> {
    let cache = JsonCache::new();
    let analyzer = ProjectAnalyzer;
    let mut projects = ensure_projects_available(rescan).await?;

    if let Some(project_filter) = project_filter {
        let matcher = SkimMatcherV2::default();
        projects.retain(|project| {
            project.name.eq_ignore_ascii_case(&project_filter)
                || matcher
                    .fuzzy_match(&project.name, &project_filter)
                    .is_some()
        });
    }

    if projects.is_empty() {
        println!("No projects available for stats.");
        return Ok(());
    }

    for project in &mut projects {
        if project.total_files.is_none() || project.total_lines.is_none() {
            let (total_files, total_lines) = analyzer.analyze_project_summary(&project.path);
            project.total_files = Some(total_files);
            project.total_lines = Some(total_lines);
        }
    }
    cache.save_projects(projects.clone()).await?;

    let git_daily_stats = GitStatsService.fetch_git_stats(&projects, range)?;
    let summary = build_stats_summary(&projects, git_daily_stats, top);

    print_kv_table(
        "Stats Summary",
        &[
            ("Projects analyzed", summary.projects_analyzed.to_string()),
            ("Tracked files", summary.tracked_files.to_string()),
            ("Tracked code lines", summary.tracked_code_lines.to_string()),
            (
                "Git activity",
                format!(
                    "+{} / -{} across {} commits",
                    summary.git_additions, summary.git_deletions, summary.git_commits
                ),
            ),
        ],
    );

    print_metrics("Top projects by lines", &summary.code_metrics);
    print_metrics("Top projects by files", &summary.file_metrics);
    print_metrics("Project types", &summary.type_metrics);

    if !summary.git_daily_stats.is_empty() {
        let git_rows = summary
            .git_daily_stats
            .iter()
            .take(top)
            .map(|entry| {
                vec![
                    entry.date.format("%Y-%m-%d").to_string(),
                    entry.project_name.clone(),
                    entry.additions.to_string(),
                    entry.deletions.to_string(),
                    entry.commits.to_string(),
                ]
            })
            .collect::<Vec<_>>();
        print_table(
            "Git Timeline",
            &["Date", "Project", "Additions", "Deletions", "Commits"],
            &git_rows,
        );
    }

    Ok(())
}

pub async fn unused_code(name: Option<String>, path: Option<String>, rescan: bool) -> Result<()> {
    let project = resolve_project(name, path, rescan).await?;
    let findings = UnusedCodeAnalyzer.analyze(&project.path);
    if findings.is_empty() {
        println!("No likely unused code found.");
        return Ok(());
    }

    let rows = findings
        .into_iter()
        .map(|finding| {
            vec![
                finding.kind,
                finding.name,
                finding.location,
                finding.hints.join(" | "),
            ]
        })
        .collect::<Vec<_>>();
    print_table(
        "Unused Code Findings",
        &["Kind", "Name", "Location", "Hints"],
        &rows,
    );
    Ok(())
}

pub async fn status() -> Result<()> {
    let cache = JsonCache::new();
    let editors = EditorDetector::new().detect_installed_editors();

    let last_indexed = match cache.get_last_indexed().await {
        Some(indexed) => format!(
            "{} ({} hours ago)",
            indexed,
            (Utc::now() - indexed).num_hours()
        ),
        None => "never".to_string(),
    };
    print_kv_table(
        "Status",
        &[
            ("Cache path", cache.path().display().to_string()),
            (
                "Projects cached",
                cache.get_project_count().await.to_string(),
            ),
            ("Cache size", format!("{} bytes", cache.get_cache_size())),
            ("Last indexed", last_indexed),
            (
                "Rescan needed",
                if cache.needs_rescan().await {
                    "yes".to_string()
                } else {
                    "no".to_string()
                },
            ),
        ],
    );
    let editor_rows = editors
        .into_iter()
        .map(|editor| {
            vec![
                editor.name,
                if editor.is_installed {
                    "yes".to_string()
                } else {
                    "no".to_string()
                },
                editor
                    .full_path
                    .unwrap_or_else(|| "not installed".to_string()),
            ]
        })
        .collect::<Vec<_>>();
    print_table("Editors", &["Name", "Installed", "Path"], &editor_rows);
    Ok(())
}

pub async fn clear_cache() -> Result<()> {
    JsonCache::new().clear().await?;
    println!("Cache cleared.");
    Ok(())
}

pub fn onboarding_status() -> Result<()> {
    let settings = SettingsStore::new().load()?;
    println!(
        "Onboarding completed: {}",
        if settings.has_completed_onboarding {
            "yes"
        } else {
            "no"
        }
    );
    Ok(())
}

pub fn onboarding_complete() -> Result<()> {
    SettingsStore::new().update(|settings| settings.has_completed_onboarding = true)?;
    println!("Onboarding marked as completed.");
    Ok(())
}

pub fn onboarding_reset() -> Result<()> {
    SettingsStore::new().update(|settings| settings.has_completed_onboarding = false)?;
    println!("Onboarding reset.");
    Ok(())
}

pub fn onboarding_tour() {
    println!("1. Welcome: DevAtlas scans your drives and builds a project inventory.");
    println!("2. Features: scan, list, open, run, analyze, dependencies, stats, unused-code.");
    println!("3. Quick Actions: open projects in VS Code, Cursor, Windsurf, or Antigravity.");
    println!("4. Stats Notebook: line counts, file counts, project types, and git activity.");
}

async fn ensure_projects_available(rescan: bool) -> Result<Vec<ProjectInfo>> {
    let cache = JsonCache::new();
    let settings = SettingsStore::new().load()?;
    if rescan || cache.needs_rescan().await {
        let scanner = ProjectScanner::new(settings.exclude_paths);
        let projects = scanner.scan_all_drives(None).await?;
        cache.save_projects(projects.clone()).await?;
        Ok(projects)
    } else {
        cache.load_projects().await
    }
}

async fn resolve_project(
    name: Option<String>,
    path: Option<String>,
    rescan: bool,
) -> Result<ProjectInfo> {
    if let Some(path) = path {
        let path = std::fs::canonicalize(&path)
            .map(|canonical| canonical.to_string_lossy().to_string())
            .unwrap_or(path);
        if let Ok(projects) = JsonCache::new().load_projects().await {
            if let Some(project) = projects
                .into_iter()
                .find(|project| project.path.eq_ignore_ascii_case(&path))
            {
                return Ok(project);
            }
        }

        if let Ok(settings) = SettingsStore::new().load() {
            let scanner = ProjectScanner::new(settings.exclude_paths);
            if let Ok(projects) = scanner.scan_path(&path, None).await {
                if let Some(project) = projects
                    .into_iter()
                    .find(|project| project.path.eq_ignore_ascii_case(&path))
                {
                    return Ok(project);
                }
            }
        }

        let name = Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string();
        return Ok(ProjectInfo {
            name,
            path,
            ..ProjectInfo::new(String::new(), String::new(), "Unknown".to_string())
        });
    }

    let projects = ensure_projects_available(rescan).await?;
    if projects.is_empty() {
        bail!("No projects found. Run `devatlas scan` first.");
    }

    if let Some(name) = name {
        return select_project_by_name(&projects, &name);
    }

    prompt_for_project(&projects)
}

fn select_project_by_name(projects: &[ProjectInfo], query: &str) -> Result<ProjectInfo> {
    if let Some(project) = projects
        .iter()
        .find(|project| project.name.eq_ignore_ascii_case(query))
    {
        return Ok(project.clone());
    }

    let matcher = SkimMatcherV2::default();
    projects
        .iter()
        .filter_map(|project| {
            matcher
                .fuzzy_match(&project.name, query)
                .map(|score| (score, project))
        })
        .max_by_key(|(score, _)| *score)
        .map(|(_, project)| project.clone())
        .with_context(|| format!("Project not found: {query}"))
}

fn rank_project_search_match(
    matcher: &SkimMatcherV2,
    project: &ProjectInfo,
    query: &str,
) -> Option<(u8, i64)> {
    if let Some(score) = matcher.fuzzy_match(&project.name, query) {
        return Some((3, i64::from(score)));
    }

    if let Some(score) = matcher.fuzzy_match(&project.category, query) {
        return Some((2, i64::from(score)));
    }

    if let Some(score) = matcher.fuzzy_match(&project.project_type, query) {
        return Some((1, i64::from(score)));
    }

    None
}

fn prompt_for_project(projects: &[ProjectInfo]) -> Result<ProjectInfo> {
    println!("Select a project:");
    print_projects_table(projects);
    print!("Enter number: ");
    stdout().flush()?;

    let mut input = String::new();
    stdin().read_line(&mut input)?;
    let selection = input.trim().parse::<usize>().context("Invalid selection")?;
    projects
        .get(selection.saturating_sub(1))
        .cloned()
        .context("Selection out of range")
}

fn prompt_for_editor(detector: &EditorDetector) -> Result<crate::models::CodeEditor> {
    let editors = detector.detect_installed_editors();
    let installed = editors
        .into_iter()
        .filter(|editor| editor.is_installed)
        .collect::<Vec<_>>();

    if installed.is_empty() {
        bail!("No supported editors found.");
    }

    println!("Select an editor:");
    let rows = installed
        .iter()
        .enumerate()
        .map(|(index, editor)| {
            vec![
                (index + 1).to_string(),
                editor.display_name.clone(),
                editor.name.clone(),
                editor
                    .full_path
                    .clone()
                    .unwrap_or_else(|| "not installed".to_string()),
            ]
        })
        .collect::<Vec<_>>();
    print_table("", &["#", "Display Name", "Name", "Path"], &rows);
    print!("Enter number: ");
    stdout().flush()?;

    let mut input = String::new();
    stdin().read_line(&mut input)?;
    let selection = input.trim().parse::<usize>().context("Invalid selection")?;
    installed
        .get(selection.saturating_sub(1))
        .cloned()
        .context("Selection out of range")
}

fn build_stats_summary(
    projects: &[ProjectInfo],
    git_daily_stats: Vec<crate::models::GitDailyStat>,
    top: usize,
) -> StatsSummary {
    let mut code_metrics = projects
        .iter()
        .filter_map(|project| {
            project.total_lines.map(|lines| ProjectMetric {
                project_name: project.name.clone(),
                project_type: project.project_type.clone(),
                value: lines,
            })
        })
        .collect::<Vec<_>>();
    code_metrics.sort_by(|left, right| right.value.cmp(&left.value));
    code_metrics.truncate(top);

    let mut file_metrics = projects
        .iter()
        .filter_map(|project| {
            project.total_files.map(|files| ProjectMetric {
                project_name: project.name.clone(),
                project_type: project.project_type.clone(),
                value: files,
            })
        })
        .collect::<Vec<_>>();
    file_metrics.sort_by(|left, right| right.value.cmp(&left.value));
    file_metrics.truncate(top);

    let mut type_counts = BTreeMap::<String, u32>::new();
    for project in projects {
        *type_counts.entry(project.project_type.clone()).or_insert(0) += 1;
    }
    let mut type_metrics = type_counts
        .into_iter()
        .map(|(project_type, value)| ProjectMetric {
            project_name: project_type.clone(),
            project_type,
            value,
        })
        .collect::<Vec<_>>();
    type_metrics.sort_by(|left, right| right.value.cmp(&left.value));
    type_metrics.truncate(top);

    let git_additions = git_daily_stats.iter().map(|entry| entry.additions).sum();
    let git_deletions = git_daily_stats.iter().map(|entry| entry.deletions).sum();
    let git_commits = git_daily_stats.iter().map(|entry| entry.commits).sum();

    let mut display_git_stats = git_daily_stats;
    display_git_stats.sort_by(|left, right| right.date.cmp(&left.date));
    display_git_stats.truncate(top);

    StatsSummary {
        projects_analyzed: projects.len(),
        tracked_code_lines: projects
            .iter()
            .map(|project| u64::from(project.total_lines.unwrap_or(0)))
            .sum(),
        tracked_files: projects
            .iter()
            .map(|project| u64::from(project.total_files.unwrap_or(0)))
            .sum(),
        git_additions,
        git_deletions,
        git_commits,
        code_metrics,
        file_metrics,
        type_metrics,
        git_daily_stats: display_git_stats,
    }
}

fn print_metrics(title: &str, metrics: &[ProjectMetric]) {
    if metrics.is_empty() {
        return;
    }

    let rows = metrics
        .iter()
        .map(|metric| {
            vec![
                metric.project_name.clone(),
                metric.project_type.clone(),
                metric.value.to_string(),
            ]
        })
        .collect::<Vec<_>>();
    print_table(title, &["Name", "Type", "Value"], &rows);
}

fn print_project_inventory(projects: &[ProjectInfo]) {
    println!("Found {} projects", projects.len());
    let mut categories = BTreeMap::<String, usize>::new();
    for project in projects {
        *categories.entry(project.category.clone()).or_insert(0) += 1;
    }
    let rows = categories
        .into_iter()
        .map(|(category, count)| vec![category, count.to_string()])
        .collect::<Vec<_>>();
    print_table("Projects by Category", &["Category", "Count"], &rows);
}

fn print_projects_table(projects: &[ProjectInfo]) {
    let rows = projects
        .iter()
        .enumerate()
        .map(|(index, project)| {
            vec![
                (index + 1).to_string(),
                project.name.clone(),
                project.category.clone(),
                project.project_type.clone(),
                project.path.clone(),
            ]
        })
        .collect::<Vec<_>>();
    print_table("", &["#", "Name", "Category", "Type", "Path"], &rows);
}

fn print_kv_table(title: &str, entries: &[(&str, String)]) {
    let rows = entries
        .iter()
        .map(|(key, value)| vec![(*key).to_string(), value.clone()])
        .collect::<Vec<_>>();
    print_table(title, &["Field", "Value"], &rows);
}

fn print_table(title: &str, headers: &[&str], rows: &[Vec<String>]) {
    if !title.is_empty() {
        println!();
        println!("{title}");
    }

    let widths = compute_widths(headers, rows);
    let border = build_border(&widths);

    println!("{border}");
    print_row(
        &headers
            .iter()
            .map(|header| (*header).to_string())
            .collect::<Vec<_>>(),
        &widths,
    );
    println!("{border}");
    for row in rows {
        print_row(row, &widths);
    }
    println!("{border}");
}

fn compute_widths(headers: &[&str], rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths = headers
        .iter()
        .map(|header| header.chars().count())
        .collect::<Vec<_>>();

    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if let Some(width) = widths.get_mut(index) {
                *width = (*width).max(cell.chars().count());
            }
        }
    }

    widths
}

fn build_border(widths: &[usize]) -> String {
    let mut border = String::from("+");
    for width in widths {
        border.push_str(&format!("-{:-<width$}-+", "", width = *width));
    }
    border
}

fn print_row(row: &[String], widths: &[usize]) {
    let mut line = String::from("|");
    for (cell, width) in row.iter().zip(widths.iter()) {
        line.push_str(&format!(" {:<width$} |", cell, width = *width));
    }
    println!("{line}");
}

fn versions_match(current: &str, latest: &str) -> bool {
    normalize_version_for_compare(current) == normalize_version_for_compare(latest)
}

fn normalize_version_for_compare(version: &str) -> String {
    version
        .trim()
        .trim_start_matches('^')
        .trim_start_matches('~')
        .trim_start_matches('=')
        .trim_start_matches('v')
        .to_ascii_lowercase()
}
