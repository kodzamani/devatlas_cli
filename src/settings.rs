use crate::models::AppSettings;
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct AppSettingsFile {
    #[serde(rename = "Language")]
    language: String,
    #[serde(rename = "AccentColor")]
    accent_color: String,
    #[serde(rename = "ThemeMode")]
    theme_mode: String,
    #[serde(rename = "HasCompletedOnboarding")]
    has_completed_onboarding: bool,
    #[serde(rename = "ExcludePaths", default)]
    exclude_paths: Vec<String>,
}

pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new() -> Self {
        let path = app_dir().join("settings.json");
        Self { path }
    }

    pub fn load(&self) -> Result<AppSettings> {
        if !self.path.exists() {
            return Ok(AppSettings::default());
        }

        let content = fs::read_to_string(&self.path)
            .with_context(|| format!("Failed to read settings file {}", self.path.display()))?;
        let file: AppSettingsFile = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse settings file {}", self.path.display()))?;

        Ok(AppSettings {
            language: file.language.parse().unwrap_or_default(),
            accent_color: file.accent_color.parse().unwrap_or_default(),
            theme_mode: file.theme_mode.parse().unwrap_or_default(),
            has_completed_onboarding: file.has_completed_onboarding,
            exclude_paths: normalize_paths(file.exclude_paths),
        })
    }

    pub fn save(&self, settings: &AppSettings) -> Result<()> {
        ensure_app_dir()?;
        let file = AppSettingsFile {
            language: settings.language.to_string(),
            accent_color: settings.accent_color.to_string(),
            theme_mode: settings.theme_mode.to_string(),
            has_completed_onboarding: settings.has_completed_onboarding,
            exclude_paths: normalize_paths(settings.exclude_paths.clone()),
        };
        let content = serde_json::to_string_pretty(&file)?;
        fs::write(&self.path, content)
            .with_context(|| format!("Failed to write settings file {}", self.path.display()))?;
        Ok(())
    }

    pub fn update<F>(&self, apply: F) -> Result<AppSettings>
    where
        F: FnOnce(&mut AppSettings),
    {
        let mut settings = self.load()?;
        apply(&mut settings);
        self.save(&settings)?;
        Ok(settings)
    }
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::new()
    }
}

pub fn app_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("DevAtlas")
}

pub fn ensure_app_dir() -> Result<PathBuf> {
    let dir = app_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create app directory {}", dir.display()))?;
    }
    Ok(dir)
}

pub fn normalize_paths(paths: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for path in paths {
        let candidate = path
            .trim()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_string();

        if candidate.is_empty() {
            continue;
        }

        let key = candidate.to_lowercase();
        if seen.insert(key) {
            normalized.push(candidate);
        }
    }

    normalized
}
