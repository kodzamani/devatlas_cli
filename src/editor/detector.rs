use crate::models::CodeEditor;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::info;

/// Editor detector that finds installed code editors
pub struct EditorDetector {
    editor_definitions: Vec<EditorDefinition>,
}

struct EditorDefinition {
    name: String,
    display_name: String,
    command: String,
    default_paths: Vec<String>,
}

impl EditorDetector {
    pub fn new() -> Self {
        let local_app_data = std::env::var("LOCALAPPDATA")
            .unwrap_or_else(|_| "C:\\Users\\Default\\AppData\\Local".to_string());

        let editor_definitions = vec![
            EditorDefinition {
                name: "vscode".to_string(),
                display_name: "VS Code".to_string(),
                command: "code".to_string(),
                default_paths: vec![format!(
                    "{}\\Programs\\Microsoft VS Code\\Code.exe",
                    local_app_data
                )],
            },
            EditorDefinition {
                name: "cursor".to_string(),
                display_name: "Cursor".to_string(),
                command: "cursor".to_string(),
                default_paths: vec![
                    format!("{}\\Cursor\\Cursor.exe", local_app_data),
                    format!("{}\\Programs\\Cursor\\Cursor.exe", local_app_data),
                ],
            },
            EditorDefinition {
                name: "windsurf".to_string(),
                display_name: "Windsurf".to_string(),
                command: "windsurf".to_string(),
                default_paths: vec![
                    format!("{}\\Windsurf\\Windsurf.exe", local_app_data),
                    format!("{}\\Programs\\Windsurf\\Windsurf.exe", local_app_data),
                ],
            },
            EditorDefinition {
                name: "antigravity".to_string(),
                display_name: "Antigravity".to_string(),
                command: "antigravity".to_string(),
                default_paths: vec![format!("{}\\Antigravity\\Antigravity.exe", local_app_data)],
            },
        ];

        Self { editor_definitions }
    }

    /// Detect all installed editors
    pub fn detect_installed_editors(&self) -> Vec<CodeEditor> {
        self.editor_definitions
            .iter()
            .map(|def| {
                let full_path = self.find_editor_path(&def.command, &def.default_paths);
                CodeEditor {
                    name: def.name.clone(),
                    display_name: def.display_name.clone(),
                    command: def.command.clone(),
                    is_installed: full_path.is_some(),
                    full_path,
                }
            })
            .collect()
    }

    /// Find editor path by checking default paths and PATH
    fn find_editor_path(&self, command: &str, default_paths: &[String]) -> Option<String> {
        for path in default_paths {
            if Path::new(path).exists() {
                return Some(path.clone());
            }
        }

        which::which(command)
            .ok()
            .map(|path| path.to_string_lossy().to_string())
            .or_else(|| self.find_via_command(command).ok())
    }

    fn find_via_command(&self, command: &str) -> Result<String> {
        let finder = if cfg!(windows) { "where" } else { "which" };
        let output = Command::new(finder).arg(command).output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(first) = stdout.lines().find(|line| !line.trim().is_empty()) {
                let candidate = PathBuf::from(first.trim());
                if candidate.exists() {
                    return Ok(candidate.to_string_lossy().to_string());
                }
            }
        }

        Err(anyhow::anyhow!("Editor not found"))
    }

    /// Get editor by name
    pub fn get_editor_by_name(&self, name: &str) -> Option<CodeEditor> {
        let editors = self.detect_installed_editors();
        editors.into_iter().find(|e| e.name == name)
    }

    /// Open project in specified editor
    pub fn open_in_editor(editor: &CodeEditor, project_path: &str) -> Result<()> {
        let full_path = editor
            .full_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Editor path not available"))?;

        #[cfg(windows)]
        {
            Command::new(full_path).arg(project_path).spawn()?;
        }

        #[cfg(not(windows))]
        {
            Command::new(&editor.command).arg(project_path).spawn()?;
        }

        info!("Opened {} in {}", project_path, editor.display_name);
        Ok(())
    }
}

impl Default for EditorDetector {
    fn default() -> Self {
        Self::new()
    }
}
