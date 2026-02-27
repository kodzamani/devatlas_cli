use chrono::{DateTime, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    #[serde(default = "uuid_v4")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub project_type: String,
    #[serde(default = "default_other")]
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "utc_now")]
    pub last_modified: DateTime<Utc>,
    #[serde(default = "utc_now")]
    pub last_indexed: DateTime<Utc>,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub icon_text: Option<String>,
    #[serde(default)]
    pub icon_color: Option<String>,
    #[serde(default)]
    pub total_lines: Option<u32>,
    #[serde(default)]
    pub total_files: Option<u32>,
}

impl ProjectInfo {
    pub fn new(path: String, name: String, project_type: String) -> Self {
        Self {
            id: uuid_v4(),
            name,
            path,
            project_type,
            category: default_other(),
            tags: Vec::new(),
            last_modified: utc_now(),
            last_indexed: utc_now(),
            is_active: false,
            git_branch: None,
            icon_text: None,
            icon_color: None,
            total_lines: None,
            total_files: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIndexData {
    #[serde(default)]
    pub projects: Vec<ProjectInfo>,
    #[serde(default = "utc_now")]
    pub last_indexed: DateTime<Utc>,
    #[serde(default = "default_index_version")]
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodeEditor {
    pub name: String,
    pub display_name: String,
    pub command: String,
    pub full_path: Option<String>,
    pub is_installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanProgress {
    pub is_scanning: bool,
    pub current_drive: String,
    pub current_path: String,
    pub total_drives: u32,
    pub processed_drives: u32,
    pub projects_found: u32,
    pub directories_scanned: u64,
    pub progress_percentage: f32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectAnalysisResult {
    pub total_files: u32,
    pub total_lines: u32,
    pub largest_file_lines: u32,
    pub largest_file_name: String,
    pub files: Vec<FileAnalysisInfo>,
    pub languages: Vec<LanguageBreakdown>,
}

impl ProjectAnalysisResult {
    pub fn avg_lines_per_file(&self) -> u32 {
        if self.total_files == 0 {
            0
        } else {
            self.total_lines / self.total_files
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileAnalysisInfo {
    pub relative_path: String,
    pub extension: String,
    pub lines: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LanguageBreakdown {
    pub name: String,
    pub extension: String,
    pub file_count: u32,
    pub total_lines: u32,
    pub percentage: f64,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TechStackItem {
    pub name: String,
    pub lines: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PackageDependency {
    pub name: String,
    pub version: String,
    pub source: String,
    pub latest_version: Option<String>,
    #[serde(default)]
    pub is_checking_update: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DependencyGroup {
    pub name: String,
    pub file_path: String,
    pub packages: Vec<PackageDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DependencySection {
    pub name: String,
    pub icon: String,
    pub groups: Vec<DependencyGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitDailyStat {
    pub date: DateTime<Utc>,
    pub project_name: String,
    pub additions: i32,
    pub deletions: i32,
    pub commits: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectMetric {
    pub project_name: String,
    pub project_type: String,
    pub value: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatsSummary {
    pub projects_analyzed: usize,
    pub tracked_code_lines: u64,
    pub tracked_files: u64,
    pub git_additions: i32,
    pub git_deletions: i32,
    pub git_commits: i32,
    pub code_metrics: Vec<ProjectMetric>,
    pub file_metrics: Vec<ProjectMetric>,
    pub type_metrics: Vec<ProjectMetric>,
    pub git_daily_stats: Vec<GitDailyStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UnusedCodeResult {
    pub kind: String,
    pub name: String,
    pub location: String,
    pub hints: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum AppLanguage {
    #[default]
    English,
    Turkish,
    German,
    Japanese,
    ChineseSimplified,
    Korean,
    Italian,
    French,
}

impl Display for AppLanguage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::English => "English",
            Self::Turkish => "Turkish",
            Self::German => "German",
            Self::Japanese => "Japanese",
            Self::ChineseSimplified => "ChineseSimplified",
            Self::Korean => "Korean",
            Self::Italian => "Italian",
            Self::French => "French",
        })
    }
}

impl FromStr for AppLanguage {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "english" | "en" => Ok(Self::English),
            "turkish" | "tr" => Ok(Self::Turkish),
            "german" | "de" => Ok(Self::German),
            "japanese" | "ja" => Ok(Self::Japanese),
            "chinesesimplified" | "zh-hans" | "chinese" => Ok(Self::ChineseSimplified),
            "korean" | "ko" => Ok(Self::Korean),
            "italian" | "it" => Ok(Self::Italian),
            "french" | "fr" => Ok(Self::French),
            other => Err(format!("Unsupported language: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum AppAccentColor {
    #[default]
    Blue,
    Purple,
    Pink,
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    Indigo,
    Cyan,
}

impl Display for AppAccentColor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Pink => "Pink",
            Self::Red => "Red",
            Self::Orange => "Orange",
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Teal => "Teal",
            Self::Indigo => "Indigo",
            Self::Cyan => "Cyan",
        })
    }
}

impl FromStr for AppAccentColor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "blue" => Ok(Self::Blue),
            "purple" => Ok(Self::Purple),
            "pink" => Ok(Self::Pink),
            "red" => Ok(Self::Red),
            "orange" => Ok(Self::Orange),
            "yellow" => Ok(Self::Yellow),
            "green" => Ok(Self::Green),
            "teal" => Ok(Self::Teal),
            "indigo" => Ok(Self::Indigo),
            "cyan" => Ok(Self::Cyan),
            other => Err(format!("Unsupported accent color: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum AppThemeMode {
    #[default]
    Light,
    Dark,
    System,
}

impl Display for AppThemeMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::System => "System",
        })
    }
}

impl FromStr for AppThemeMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            "system" => Ok(Self::System),
            other => Err(format!("Unsupported theme mode: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum DateRangeFilter {
    Week,
    #[default]
    Month,
    Year,
    AllTime,
}

impl DateRangeFilter {
    pub fn days(self) -> Option<i64> {
        match self {
            Self::Week => Some(7),
            Self::Month => Some(30),
            Self::Year => Some(365),
            Self::AllTime => None,
        }
    }
}

impl Display for DateRangeFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Week => "Week",
            Self::Month => "Month",
            Self::Year => "Year",
            Self::AllTime => "AllTime",
        })
    }
}

impl FromStr for DateRangeFilter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "week" | "7days" => Ok(Self::Week),
            "month" | "30days" => Ok(Self::Month),
            "year" => Ok(Self::Year),
            "alltime" | "all-time" | "all" => Ok(Self::AllTime),
            other => Err(format!("Unsupported date range: {other}")),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AppSettings {
    pub language: AppLanguage,
    pub accent_color: AppAccentColor,
    pub theme_mode: AppThemeMode,
    pub has_completed_onboarding: bool,
    pub exclude_paths: Vec<String>,
}

pub fn utc_now() -> DateTime<Utc> {
    Utc::now()
}

fn default_other() -> String {
    "Other".to_string()
}

fn default_index_version() -> u32 {
    1
}

fn uuid_v4() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let timestamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos() as u64,
        Err(_) => 0,
    };
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let process_id = u64::from(std::process::id());
    let id = timestamp
        .wrapping_mul(31)
        .wrapping_add(process_id.wrapping_mul(17))
        .wrapping_add(counter);

    format!("{id:032x}")
}
