use crate::models::UnusedCodeResult;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

pub struct UnusedCodeAnalyzer;

impl UnusedCodeAnalyzer {
    pub fn analyze(&self, project_path: &str) -> Vec<UnusedCodeResult> {
        let primary_language = detect_primary_language(project_path);
        match primary_language.as_str() {
            "C#" => analyze_language(
                project_path,
                &["cs"],
                &[
                    ("class", r"\bclass\s+([A-Z][A-Za-z0-9_]*)"),
                    (
                        "method",
                        r"\b(?:public|private|protected|internal|static|async|\s)+[A-Za-z0-9_<>\[\]\?]+\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(",
                    ),
                ],
            ),
            "JavaScript" => analyze_language(
                project_path,
                &["js", "jsx", "ts", "tsx", "mjs", "cjs"],
                &[
                    ("function", r"\bfunction\s+([A-Za-z_][A-Za-z0-9_]*)"),
                    ("constant", r"\bconst\s+([A-Za-z_][A-Za-z0-9_]*)\s*="),
                    ("class", r"\bclass\s+([A-Z][A-Za-z0-9_]*)"),
                ],
            ),
            "Dart" => analyze_language(
                project_path,
                &["dart"],
                &[
                    ("class", r"\bclass\s+([A-Z][A-Za-z0-9_]*)"),
                    (
                        "function",
                        r"\b(?:void|Future<.*?>|Stream<.*?>|[A-Za-z_][A-Za-z0-9_<>\?]*)\s+([a-zA-Z_][A-Za-z0-9_]*)\s*\(",
                    ),
                ],
            ),
            "Swift" => analyze_language(
                project_path,
                &["swift"],
                &[
                    ("class", r"\b(?:class|struct|enum)\s+([A-Z][A-Za-z0-9_]*)"),
                    ("function", r"\bfunc\s+([A-Za-z_][A-Za-z0-9_]*)\s*\("),
                ],
            ),
            _ => Vec::new(),
        }
    }
}

impl Default for UnusedCodeAnalyzer {
    fn default() -> Self {
        Self
    }
}

fn analyze_language(
    project_path: &str,
    extensions: &[&str],
    patterns: &[(&str, &str)],
) -> Vec<UnusedCodeResult> {
    let files = collect_source_files(project_path, extensions);
    let mut content_by_file = HashMap::<String, String>::new();
    let mut declarations = Vec::<(String, String, String)>::new();

    for path in files {
        if let Ok(content) = fs::read_to_string(&path) {
            let path_string = path.to_string_lossy().to_string();
            for (kind, pattern) in patterns {
                if let Ok(regex) = Regex::new(pattern) {
                    for capture in regex.captures_iter(&content) {
                        let Some(name) = capture.get(1).map(|value| value.as_str().to_string())
                        else {
                            continue;
                        };
                        if should_ignore_symbol(&name) {
                            continue;
                        }
                        declarations.push((kind.to_string(), name, path_string.clone()));
                    }
                }
            }
            content_by_file.insert(path_string, content);
        }
    }

    let joined = content_by_file
        .values()
        .map(std::string::String::as_str)
        .collect::<Vec<_>>()
        .join("\n");

    declarations
        .into_iter()
        .filter_map(|(kind, name, location)| {
            let regex = Regex::new(&format!(r"\b{}\b", regex::escape(&name))).ok()?;
            let occurrences = regex.find_iter(&joined).count();
            (occurrences <= 1).then_some(UnusedCodeResult {
                kind,
                name: name.clone(),
                location,
                hints: vec![
                    "Only one textual reference found in project".to_string(),
                    "Review dynamic usages before deleting".to_string(),
                ],
            })
        })
        .take(100)
        .collect()
}

fn detect_primary_language(project_path: &str) -> String {
    let mut counts = HashMap::<&str, usize>::new();
    for path in WalkDir::new(project_path)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
    {
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
        {
            "swift" => *counts.entry("Swift").or_insert(0) += 1,
            "cs" => *counts.entry("C#").or_insert(0) += 1,
            "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" => {
                *counts.entry("JavaScript").or_insert(0) += 1
            }
            "dart" => *counts.entry("Dart").or_insert(0) += 1,
            _ => {}
        }
    }

    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(language, _)| language.to_string())
        .unwrap_or_else(|| "C#".to_string())
}

fn collect_source_files(project_path: &str, extensions: &[&str]) -> Vec<std::path::PathBuf> {
    WalkDir::new(project_path)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| {
                    extensions
                        .iter()
                        .any(|allowed| allowed.eq_ignore_ascii_case(ext))
                })
                .unwrap_or(false)
        })
        .filter(|path| !contains_skip_segment(path))
        .collect()
}

fn contains_skip_segment(path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy().to_lowercase();
        [
            "node_modules",
            ".git",
            "bin",
            "obj",
            "build",
            "dist",
            ".next",
            ".nuxt",
            "pods",
            ".build",
            "vendor",
            ".cache",
            "deriveddata",
            ".dart_tool",
            ".pub-cache",
            "android",
            "ios",
            "web",
            "linux",
            "macos",
            "windows",
        ]
        .contains(&value.as_str())
    })
}

fn should_ignore_symbol(name: &str) -> bool {
    matches!(
        name,
        "main" | "Main" | "Program" | "App" | "init" | "setUp" | "tearDown"
    )
}
