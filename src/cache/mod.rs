use crate::models::{ProjectIndexData, ProjectInfo};
use crate::scanner::ProjectScanner;
use crate::settings::{app_dir, ensure_app_dir};
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use std::fs;
use std::path::PathBuf;

const CACHE_EXPIRY_HOURS: i64 = 24;

pub struct JsonCache {
    cache_path: PathBuf,
}

impl JsonCache {
    pub fn new() -> Self {
        Self {
            cache_path: app_dir().join("project_index.json"),
        }
    }

    pub fn path(&self) -> &PathBuf {
        &self.cache_path
    }

    pub async fn load_index(&self) -> Result<ProjectIndexData> {
        if !self.cache_path.exists() {
            return Ok(ProjectIndexData::default());
        }

        let content = fs::read_to_string(&self.cache_path)
            .with_context(|| format!("Failed to read cache file {}", self.cache_path.display()))?;
        let index = serde_json::from_str::<ProjectIndexData>(&content)
            .with_context(|| format!("Failed to parse cache file {}", self.cache_path.display()))?;

        Ok(index)
    }

    pub async fn load_projects(&self) -> Result<Vec<ProjectInfo>> {
        let projects = self.load_index().await?.projects;
        Ok(ProjectScanner::default().sanitize_projects(projects))
    }

    pub async fn save_projects(&self, projects: Vec<ProjectInfo>) -> Result<()> {
        ensure_app_dir()?;

        let mut normalized = projects;
        let now = Utc::now();
        for project in &mut normalized {
            project.last_indexed = now;
        }

        let index = ProjectIndexData {
            projects: normalized,
            last_indexed: now,
            version: 1,
        };
        let content = serde_json::to_string(&index)?;
        fs::write(&self.cache_path, content)
            .with_context(|| format!("Failed to write cache file {}", self.cache_path.display()))?;
        Ok(())
    }

    pub async fn clear(&self) -> Result<()> {
        if self.cache_path.exists() {
            fs::remove_file(&self.cache_path).with_context(|| {
                format!("Failed to remove cache file {}", self.cache_path.display())
            })?;
        }
        Ok(())
    }

    pub async fn needs_rescan(&self) -> bool {
        match self.load_index().await {
            Ok(index) => {
                if index.projects.is_empty() {
                    return true;
                }

                Utc::now() - index.last_indexed >= Duration::hours(CACHE_EXPIRY_HOURS)
            }
            Err(_) => true,
        }
    }

    pub async fn get_last_indexed(&self) -> Option<DateTime<Utc>> {
        match self.load_index().await {
            Ok(index) if !index.projects.is_empty() => Some(index.last_indexed),
            Ok(_) => None,
            Err(_) => None,
        }
    }

    pub async fn get_project_count(&self) -> usize {
        self.load_projects()
            .await
            .map(|projects| projects.len())
            .unwrap_or(0)
    }

    pub fn get_cache_size(&self) -> u64 {
        if self.cache_path.exists() {
            fs::metadata(&self.cache_path)
                .map(|meta| meta.len())
                .unwrap_or(0)
        } else {
            0
        }
    }

    pub async fn save_single_project_metrics(
        &self,
        project_path: &str,
        total_files: u32,
        total_lines: u32,
    ) -> Result<()> {
        let mut projects = self.load_projects().await?;
        if let Some(project) = projects
            .iter_mut()
            .find(|project| project.path.eq_ignore_ascii_case(project_path))
        {
            project.total_files = Some(total_files);
            project.total_lines = Some(total_lines);
        }
        self.save_projects(projects).await
    }
}

impl Default for JsonCache {
    fn default() -> Self {
        Self::new()
    }
}
