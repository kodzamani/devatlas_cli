use anyhow::{bail, Result};
use serde_json::Value as JsonValue;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct NpmScript {
    pub name: String,
}

pub struct ProjectRunner;

impl ProjectRunner {
    pub fn has_node_modules(project_path: &str) -> bool {
        Path::new(project_path).join("node_modules").is_dir()
    }

    pub fn get_all_scripts(project_path: &str) -> Result<Vec<NpmScript>> {
        let package_json = Path::new(project_path).join("package.json");
        if !package_json.exists() {
            return Ok(Vec::new());
        }

        let json: JsonValue = serde_json::from_str(&std::fs::read_to_string(package_json)?)?;
        Ok(json["scripts"]
            .as_object()
            .map(|scripts| {
                scripts
                    .iter()
                    .map(|(name, _value)| NpmScript { name: name.clone() })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn get_start_command(project_path: &str) -> Result<Option<String>> {
        let scripts = Self::get_all_scripts(project_path)?;
        Ok(["dev", "start", "serve"].iter().find_map(|candidate| {
            scripts
                .iter()
                .find(|script| script.name.eq_ignore_ascii_case(candidate))
                .map(|script| script.name.clone())
        }))
    }

    pub fn install(project_path: &str) -> Result<()> {
        run_shell_command(project_path, "npm install", false, false)
    }

    pub fn run(project_path: &str, script_or_command: &str, detached: bool) -> Result<()> {
        let command = normalize_script_command(script_or_command);
        run_shell_command(project_path, &command, detached, true)
    }

    pub fn open_browser(url: &str) -> Result<()> {
        #[cfg(windows)]
        {
            Command::new("cmd").args(["/c", "start", "", url]).spawn()?;
        }

        #[cfg(not(windows))]
        {
            Command::new("xdg-open").arg(url).spawn()?;
        }

        Ok(())
    }
}

fn normalize_script_command(script_or_command: &str) -> String {
    if script_or_command.starts_with("npm ")
        || script_or_command.starts_with("pnpm ")
        || script_or_command.starts_with("yarn ")
        || script_or_command.starts_with("bun ")
    {
        script_or_command.to_string()
    } else {
        format!("npm run {script_or_command}")
    }
}

fn run_shell_command(
    project_path: &str,
    command: &str,
    detached: bool,
    inherit_stdio: bool,
) -> Result<()> {
    if !Path::new(project_path).is_dir() {
        bail!("Project path does not exist: {project_path}");
    }

    #[cfg(windows)]
    let mut process = {
        let mut process = Command::new("cmd");
        if detached {
            process.args(["/k", command]);
        } else {
            process.args(["/c", command]);
        }
        process.current_dir(project_path);
        process
    };

    #[cfg(not(windows))]
    let mut process = {
        let mut process = Command::new("/bin/bash");
        process.args(["-lc", command]).current_dir(project_path);
        process
    };

    if inherit_stdio {
        process
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
    }

    if detached {
        process.spawn()?;
        Ok(())
    } else {
        let status = process.status()?;
        if status.success() {
            Ok(())
        } else {
            bail!("Command failed with exit code {:?}", status.code())
        }
    }
}
