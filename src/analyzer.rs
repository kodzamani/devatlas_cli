use crate::models::{FileAnalysisInfo, LanguageBreakdown, ProjectAnalysisResult, TechStackItem};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use walkdir::{DirEntry, WalkDir};

pub struct ProjectAnalyzer;

impl ProjectAnalyzer {
    pub fn analyze_project(&self, project_path: &str) -> ProjectAnalysisResult {
        let mut result = ProjectAnalysisResult::default();
        let mut language_stats = HashMap::<String, (u32, u32)>::new();

        if !Path::new(project_path).is_dir() {
            return result;
        }

        for file_path in self.source_files(project_path) {
            let extension = file_path
                .extension()
                .map(|ext| format!(".{}", ext.to_string_lossy().to_lowercase()))
                .unwrap_or_default();

            if !should_analyze_file(&file_path, &extension) {
                continue;
            }

            let line_count = count_lines(&file_path);
            if line_count == 0 {
                continue;
            }

            let relative_path = relative_path(project_path, &file_path);
            let language = extension_to_language(&extension);

            result.files.push(FileAnalysisInfo {
                relative_path: relative_path.clone(),
                extension: extension.trim_start_matches('.').to_string(),
                lines: line_count,
            });

            let stats = language_stats.entry(language.clone()).or_insert((0, 0));
            stats.0 += 1;
            stats.1 += line_count;

            result.total_files += 1;
            result.total_lines += line_count;

            if line_count > result.largest_file_lines {
                result.largest_file_lines = line_count;
                result.largest_file_name = relative_path;
            }
        }

        result
            .files
            .sort_by(|left, right| right.lines.cmp(&left.lines));
        result.languages = language_stats
            .into_iter()
            .map(|(name, (file_count, total_lines))| LanguageBreakdown {
                extension: name.clone(),
                percentage: if result.total_lines == 0 {
                    0.0
                } else {
                    ((total_lines as f64 / result.total_lines as f64) * 1000.0).round() / 10.0
                },
                color: language_color(&name).to_string(),
                name,
                file_count,
                total_lines,
            })
            .collect();
        result
            .languages
            .sort_by(|left, right| right.total_lines.cmp(&left.total_lines));

        result
    }

    pub fn analyze_project_summary(&self, project_path: &str) -> (u32, u32) {
        let analysis = self.analyze_project(project_path);
        (analysis.total_files, analysis.total_lines)
    }

    pub fn tech_stack_with_lines(&self, project_path: &str, tags: &[String]) -> Vec<TechStackItem> {
        let mut extension_line_count = BTreeMap::<String, u32>::new();
        for file_path in self.source_files(project_path) {
            let extension = file_path
                .extension()
                .map(|ext| format!(".{}", ext.to_string_lossy().to_lowercase()))
                .unwrap_or_default();

            if !should_analyze_file(&file_path, &extension) {
                continue;
            }

            let line_count = count_lines(&file_path);
            if line_count == 0 {
                continue;
            }

            *extension_line_count.entry(extension).or_insert(0) += line_count;
        }

        tags.iter()
            .map(|tag| TechStackItem {
                name: tag.clone(),
                lines: extensions_for_tag(tag)
                    .into_iter()
                    .map(|extension| extension_line_count.get(extension).copied().unwrap_or(0))
                    .sum(),
            })
            .collect()
    }

    fn source_files(&self, root_path: &str) -> Vec<std::path::PathBuf> {
        WalkDir::new(root_path)
            .into_iter()
            .filter_entry(allow_entry)
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| entry.into_path())
            .collect()
    }
}

impl Default for ProjectAnalyzer {
    fn default() -> Self {
        Self
    }
}

fn allow_entry(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }

    match entry.file_name().to_str() {
        Some(name) => {
            let lowered = name.to_lowercase();
            !excluded_directories().contains(lowered.as_str()) && !lowered.starts_with('.')
        }
        None => true,
    }
}

fn excluded_directories() -> HashSet<&'static str> {
    HashSet::from([
        "node_modules",
        "bin",
        "obj",
        ".git",
        ".vs",
        ".vscode",
        ".idea",
        "packages",
        "dist",
        "build",
        "out",
        "output",
        "target",
        "vendor",
        "__pycache__",
        ".mypy_cache",
        ".pytest_cache",
        "venv",
        ".venv",
        "env",
        ".env",
        ".tox",
        "coverage",
        ".next",
        ".nuxt",
        ".svelte-kit",
        ".angular",
        ".gradle",
        "gradle",
        ".dart_tool",
        ".pub-cache",
        "pods",
        "deriveddata",
        "debug",
        "release",
        "x64",
        "x86",
        "testresults",
        ".parcel-cache",
        ".cache",
        "tmp",
        "temp",
        "logs",
    ])
}

fn should_analyze_file(path: &Path, extension: &str) -> bool {
    if excluded_extensions().contains(extension) {
        return false;
    }

    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    !file_name.ends_with(".min.js")
        && !file_name.ends_with(".min.css")
        && !file_name.ends_with(".map")
}

fn excluded_extensions() -> HashSet<&'static str> {
    HashSet::from([
        ".json",
        ".lock",
        ".sum",
        ".exe",
        ".dll",
        ".pdb",
        ".so",
        ".dylib",
        ".class",
        ".jar",
        ".war",
        ".ear",
        ".zip",
        ".tar",
        ".gz",
        ".rar",
        ".7z",
        ".png",
        ".jpg",
        ".jpeg",
        ".gif",
        ".bmp",
        ".ico",
        ".svg",
        ".webp",
        ".mp3",
        ".mp4",
        ".wav",
        ".avi",
        ".mov",
        ".ttf",
        ".woff",
        ".woff2",
        ".eot",
        ".otf",
        ".pdf",
        ".doc",
        ".docx",
        ".xls",
        ".xlsx",
        ".ppt",
        ".pptx",
        ".db",
        ".sqlite",
        ".sqlite3",
        ".mdf",
        ".ldf",
        ".suo",
        ".user",
        ".cache",
        ".log",
        ".ds_store",
        ".pbxproj",
    ])
}

fn count_lines(path: &Path) -> u32 {
    match File::open(path) {
        Ok(file) => BufReader::new(file).lines().count() as u32,
        Err(_) => 0,
    }
}

fn relative_path(root: &str, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}

fn extension_to_language(extension: &str) -> String {
    match extension {
        ".cs" | ".csx" => "C#".to_string(),
        ".js" | ".jsx" | ".mjs" | ".cjs" => "JavaScript".to_string(),
        ".ts" | ".tsx" => "TypeScript".to_string(),
        ".html" | ".htm" => "HTML".to_string(),
        ".css" => "CSS".to_string(),
        ".scss" => "SCSS".to_string(),
        ".sass" => "SASS".to_string(),
        ".less" => "LESS".to_string(),
        ".py" | ".pyw" | ".pyi" => "Python".to_string(),
        ".java" => "Java".to_string(),
        ".kt" | ".kts" => "Kotlin".to_string(),
        ".c" | ".h" => "C".to_string(),
        ".cpp" | ".hpp" | ".cc" | ".cxx" => "C++".to_string(),
        ".go" => "Go".to_string(),
        ".rs" => "Rust".to_string(),
        ".rb" | ".erb" => "Ruby".to_string(),
        ".php" => "PHP".to_string(),
        ".swift" => "Swift".to_string(),
        ".dart" => "Dart".to_string(),
        ".sh" | ".bash" | ".zsh" => "Shell".to_string(),
        ".ps1" | ".psm1" => "PowerShell".to_string(),
        ".xml" => "XML".to_string(),
        ".xaml" => "XAML".to_string(),
        ".yml" | ".yaml" => "YAML".to_string(),
        ".toml" => "TOML".to_string(),
        ".sql" => "SQL".to_string(),
        ".md" | ".mdx" => "Markdown".to_string(),
        ".vue" => "Vue".to_string(),
        ".svelte" => "Svelte".to_string(),
        ".r" => "R".to_string(),
        ".lua" => "Lua".to_string(),
        ".scala" => "Scala".to_string(),
        ".ex" | ".exs" => "Elixir".to_string(),
        ".hs" => "Haskell".to_string(),
        ".dockerfile" => "Dockerfile".to_string(),
        ".proto" => "Protobuf".to_string(),
        ".graphql" | ".gql" => "GraphQL".to_string(),
        ".razor" | ".cshtml" => "Razor".to_string(),
        _ => extension.trim_start_matches('.').to_uppercase(),
    }
}

fn language_color(language: &str) -> &'static str {
    match language {
        "C#" => "#178600",
        "JavaScript" => "#F1E05A",
        "TypeScript" => "#3178C6",
        "HTML" => "#E34C26",
        "CSS" => "#563D7C",
        "SCSS" => "#C6538C",
        "SASS" => "#A2006D",
        "LESS" => "#1D365D",
        "Python" => "#3572A5",
        "Java" => "#B07219",
        "Kotlin" => "#A97BFF",
        "C" => "#555555",
        "C++" => "#F34B7D",
        "Go" => "#00ADD8",
        "Rust" => "#DEA584",
        "Ruby" => "#701516",
        "PHP" => "#4F5D95",
        "Swift" => "#F05138",
        "Dart" => "#00B4AB",
        "Shell" => "#89E051",
        "PowerShell" => "#012456",
        "XML" => "#0060AC",
        "XAML" => "#0C54C2",
        "YAML" => "#CB171E",
        "SQL" => "#E38C00",
        "Markdown" => "#083FA1",
        "Vue" => "#41B883",
        "Svelte" => "#FF3E00",
        "Razor" => "#512BD4",
        "Dockerfile" => "#384D54",
        "TOML" => "#9C4221",
        "R" => "#198CE7",
        "Lua" => "#000080",
        "Scala" => "#C22D40",
        "Elixir" => "#6E4A7E",
        "Haskell" => "#5E5086",
        "Protobuf" => "#5A5A5A",
        "GraphQL" => "#E10098",
        _ => "#6B7280",
    }
}

fn extensions_for_tag(tag: &str) -> Vec<&'static str> {
    match tag.to_lowercase().as_str() {
        "react" | "next.js" => vec![".jsx", ".tsx", ".js", ".ts"],
        "vue" => vec![".vue", ".js", ".ts"],
        "angular" | "node.js" => vec![".js", ".ts"],
        "wpf" => vec![".cs", ".xaml"],
        "winforms" => vec![".cs"],
        "asp.net core" | "blazor" => vec![".cs", ".cshtml", ".razor"],
        "flutter" | "dart" => vec![".dart"],
        "spring boot" => vec![".java", ".kt"],
        "django" | "flask" | "python" => vec![".py"],
        "rails" | "ruby" => vec![".rb", ".erb"],
        "laravel" | "php" => vec![".php"],
        "typescript" => vec![".ts", ".tsx"],
        "javascript" => vec![".js", ".jsx", ".mjs", ".cjs"],
        "go" => vec![".go"],
        "rust" => vec![".rs"],
        "java" => vec![".java"],
        "kotlin" => vec![".kt", ".kts"],
        "html" => vec![".html", ".htm"],
        "css" | "tailwind css" => vec![".css", ".scss", ".sass"],
        "scss" | "sass" => vec![".scss", ".sass"],
        "docker" => vec![".dockerfile"],
        _ => Vec::new(),
    }
}
