use crate::models::{Project, ProjectIndex};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info};

const MAX_CACHE_SIZE: usize = 200;
const CACHE_EXPIRY_HOURS: i64 = 24;

/// JSON-based cache for storing project index
pub struct JsonCache {
    cache_path: PathBuf,
}

impl JsonCache {
    pub fn new() -> Self {
        let app_data = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("DevAtlas");

        // Create directory if it doesn't exist
        if !app_data.exists() {
            let _ = fs::create_dir_all(&app_data);
        }

        let cache_path = app_data.join("project_index.json");

        Self { cache_path }
    }

    /// Get cache path
    pub fn get_cache_path(&self) -> &PathBuf {
        &self.cache_path
    }

    /// Load projects from cache
    pub async fn load_projects(&self) -> Result<Vec<Project>> {
        if !self.cache_path.exists() {
            debug!("Cache file does not exist, returning empty list");
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.cache_path)?;
        
        let index: ProjectIndex = serde_json::from_str(&content)?;
        
        debug!("Loaded {} projects from cache", index.projects.len());
        Ok(index.projects)
    }

    /// Save projects to cache
    pub async fn save_projects(&self, projects: Vec<Project>) -> Result<()> {
        // Limit cache size
        let projects_to_save = if projects.len() > MAX_CACHE_SIZE {
            info!("Caching only first {} of {} projects", MAX_CACHE_SIZE, projects.len());
            projects.into_iter().take(MAX_CACHE_SIZE).collect()
        } else {
            projects
        };

        let index = ProjectIndex {
            projects: projects_to_save,
            last_indexed: Utc::now(),
            version: 1,
        };

        let content = serde_json::to_string_pretty(&index)?;
        fs::write(&self.cache_path, content)?;

        info!("Saved {} projects to cache", index.projects.len());
        Ok(())
    }

    /// Check if cache needs rescan
    pub async fn needs_rescan(&self) -> bool {
        if !self.cache_path.exists() {
            return true;
        }

        match fs::read_to_string(&self.cache_path) {
            Ok(content) => {
                match serde_json::from_str::<ProjectIndex>(&content) {
                    Ok(index) => {
                        let hours_since_index = (Utc::now() - index.last_indexed).num_hours();
                        debug!("Last indexed {} hours ago", hours_since_index);
                        hours_since_index >= CACHE_EXPIRY_HOURS
                    }
                    Err(_) => true,
                }
            }
            Err(_) => true,
        }
    }

    /// Get last indexed time
    pub async fn get_last_indexed(&self) -> Option<DateTime<Utc>> {
        if !self.cache_path.exists() {
            return None;
        }

        match fs::read_to_string(&self.cache_path) {
            Ok(content) => {
                match serde_json::from_str::<ProjectIndex>(&content) {
                    Ok(index) => Some(index.last_indexed),
                    Err(_) => None,
                }
            }
            Err(_) => None,
        }
    }

    /// Get project count
    pub async fn get_project_count(&self) -> usize {
        match self.load_projects().await {
            Ok(projects) => projects.len(),
            Err(_) => 0,
        }
    }

    /// Clear cache
    pub async fn clear(&self) -> Result<()> {
        if self.cache_path.exists() {
            fs::remove_file(&self.cache_path)?;
            info!("Cache cleared");
        }
        Ok(())
    }

    /// Get cache size in bytes
    pub fn get_cache_size(&self) -> u64 {
        if self.cache_path.exists() {
            fs::metadata(&self.cache_path)
                .map(|m| m.len())
                .unwrap_or(0)
        } else {
            0
        }
    }
}

impl Default for JsonCache {
    fn default() -> Self {
        Self::new()
    }
}
