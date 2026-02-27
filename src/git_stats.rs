use crate::models::{DateRangeFilter, GitDailyStat, ProjectInfo};
use anyhow::Result;
use chrono::{NaiveDate, TimeZone, Utc};
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

pub struct GitStatsService;

impl GitStatsService {
    pub fn fetch_git_stats(
        &self,
        projects: &[ProjectInfo],
        range: DateRangeFilter,
    ) -> Result<Vec<GitDailyStat>> {
        let Some(git_path) = find_git_executable() else {
            return Ok(Vec::new());
        };

        let cutoff = range
            .days()
            .map(|days| Utc::now() - chrono::Duration::days(days));
        let mut all_stats = Vec::new();

        for project in projects
            .iter()
            .filter(|project| is_git_repository(&project.path))
        {
            let mut project_stats =
                fetch_project_stats(&git_path, &project.path, &project.name, cutoff)?;
            all_stats.append(&mut project_stats);
        }

        all_stats.sort_by(|left, right| left.date.cmp(&right.date));
        Ok(all_stats)
    }
}

impl Default for GitStatsService {
    fn default() -> Self {
        Self
    }
}

fn fetch_project_stats(
    git_path: &str,
    project_path: &str,
    project_name: &str,
    since: Option<chrono::DateTime<Utc>>,
) -> Result<Vec<GitDailyStat>> {
    let mut args = vec![
        "log".to_string(),
        "--numstat".to_string(),
        "--date=short".to_string(),
        "--format=%ad".to_string(),
    ];
    if let Some(since) = since {
        args.push(format!("--since={}", since.format("%Y-%m-%d")));
    }

    let output = Command::new(git_path)
        .args(args)
        .current_dir(project_path)
        .output()?;
    if !output.status.success() {
        return Ok(Vec::new());
    }

    let date_regex = Regex::new(r"^\d{4}-\d{2}-\d{2}$")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut current_date = String::new();
    let mut stats = HashMap::<String, (i32, i32, i32)>::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if date_regex.is_match(trimmed) {
            current_date = trimmed.to_string();
            let entry = stats.entry(current_date.clone()).or_insert((0, 0, 0));
            entry.2 += 1;
            continue;
        }

        let parts = trimmed.split('\t').collect::<Vec<_>>();
        if parts.len() >= 2 && parts[0] != "-" && parts[1] != "-" && !current_date.is_empty() {
            let added = parts[0].parse::<i32>().unwrap_or(0);
            let deleted = parts[1].parse::<i32>().unwrap_or(0);
            let entry = stats.entry(current_date.clone()).or_insert((0, 0, 0));
            entry.0 += added;
            entry.1 += deleted;
        }
    }

    Ok(stats
        .into_iter()
        .filter_map(|(date, (additions, deletions, commits))| {
            let naive = NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok()?;
            let date = Utc.from_utc_datetime(&naive.and_hms_opt(0, 0, 0)?);
            Some(GitDailyStat {
                date,
                project_name: project_name.to_string(),
                additions,
                deletions,
                commits,
            })
        })
        .collect())
}

fn is_git_repository(project_path: &str) -> bool {
    let path = Path::new(project_path).join(".git");
    path.is_dir() || path.is_file()
}

fn find_git_executable() -> Option<String> {
    which::which("git")
        .ok()
        .map(|path| path.to_string_lossy().to_string())
        .or_else(|| {
            [
                r"C:\Program Files\Git\bin\git.exe",
                r"C:\Program Files (x86)\Git\bin\git.exe",
            ]
            .iter()
            .find(|candidate| Path::new(candidate).exists())
            .map(|candidate| candidate.to_string())
        })
}
