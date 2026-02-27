use crate::models::{ProjectInfo, ScanProgress};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use tracing::info;
use walkdir::{DirEntry, WalkDir};

#[cfg(windows)]
use std::path::PathBuf;

const MAX_SCAN_DEPTH: usize = 10;

pub struct ProjectScanner {
    exclude_paths: Vec<String>,
    skip_directories: HashSet<String>,
    skip_unix_root_directories: HashSet<String>,
    extension_markers: HashSet<String>,
    filename_markers: HashSet<String>,
    project_type_by_extension: HashMap<String, String>,
    project_type_by_filename: HashMap<String, String>,
    technology_tags: HashMap<String, Vec<String>>,
    web_project_types: HashSet<String>,
    desktop_project_types: HashSet<String>,
    mobile_project_types: HashSet<String>,
    cloud_project_types: HashSet<String>,
    web_project_files: HashSet<String>,
    desktop_project_files: HashSet<String>,
    mobile_project_files: HashSet<String>,
    cloud_project_files: HashSet<String>,
}

impl ProjectScanner {
    pub fn new(mut exclude_paths: Vec<String>) -> Self {
        exclude_paths.extend(default_exclude_paths());
        exclude_paths = normalize_exclude_paths(exclude_paths);

        Self {
            exclude_paths,
            skip_directories: set(&[
                "node_modules",
                "bin",
                "obj",
                ".svn",
                ".hg",
                "windows",
                "program files",
                "program files (x86)",
                "programdata",
                "$recycle.bin",
                "system volume information",
                "windows.old",
                "perflogs",
                ".nuget",
                "packages",
                ".vs",
                ".idea",
                ".vscode",
                "dist",
                "out",
                "target",
                "vendor",
                "__pycache__",
                ".venv",
                "venv",
                "env",
                ".tox",
                ".mypy_cache",
                ".pytest_cache",
                ".cache",
                ".tmp",
                "temp",
                "tmp",
                "recovery",
                "msocache",
                "intel",
                "nvidia",
                "amd",
                "drivers",
                "boot",
                "go",
                "plugins",
                "pkg",
                "flutter",
                "library",
                "applications",
                "xcode",
                "deriveddata",
            ]),
            skip_unix_root_directories: set(&[
                "applications",
                "cores",
                "dev",
                "etc",
                "lib",
                "lib64",
                "library",
                "lost+found",
                "private",
                "proc",
                "root",
                "run",
                "opt",
                "sbin",
                "snap",
                "sys",
                "system",
                "usr",
                "var",
                "volumes",
                "Library",
                "System",
                "Applications",
                "Xcode",
                "DerivedData",
            ]),
            extension_markers: set(&[".csproj", ".fsproj", ".vbproj", ".sln", ".slnx"]),
            filename_markers: set(&[
                "package.json",
                "requirements.txt",
                "setup.py",
                "pyproject.toml",
                "Pipfile",
                "main.py",
                "app.py",
                "__init__.py",
                "__main__.py",
                "pom.xml",
                "build.gradle",
                "build.gradle.kts",
                "go.mod",
                "Cargo.toml",
                "composer.json",
                "Gemfile",
                "Package.swift",
                "Podfile",
                "pubspec.yaml",
                "next.config.js",
                "next.config.mjs",
                "next.config.ts",
                "vue.config.js",
                "vite.config.js",
                "vite.config.ts",
                "angular.json",
                "Dockerfile",
                "docker-compose.yml",
                "docker-compose.yaml",
            ]),
            project_type_by_extension: map(&[
                (".csproj", ".NET"),
                (".fsproj", "F#"),
                (".vbproj", "VB.NET"),
                (".sln", ".NET Solution"),
                (".slnx", ".NET Solution"),
            ]),
            project_type_by_filename: map(&[
                ("package.json", "Node.js"),
                ("go.mod", "Go"),
                ("Cargo.toml", "Rust"),
                ("pom.xml", "Java/Maven"),
                ("build.gradle", "Java/Gradle"),
                ("build.gradle.kts", "Java/Gradle"),
                ("composer.json", "PHP"),
                ("Gemfile", "Ruby"),
                ("requirements.txt", "Python"),
                ("pyproject.toml", "Python"),
                ("Pipfile", "Python"),
                ("setup.py", "Python"),
                ("main.py", "Python"),
                ("app.py", "Python"),
                ("__init__.py", "Python"),
                ("__main__.py", "Python"),
                ("pubspec.yaml", "Flutter"),
                ("Podfile", "iOS"),
                ("Package.swift", "Swift"),
                ("angular.json", "Angular"),
                ("next.config.js", "Next.js"),
                ("next.config.mjs", "Next.js"),
                ("next.config.ts", "Next.js"),
                ("vue.config.js", "Vue"),
                ("vite.config.js", "Vite"),
                ("vite.config.ts", "Vite"),
                ("Dockerfile", "Docker"),
                ("docker-compose.yml", "Docker"),
                ("docker-compose.yaml", "Docker"),
            ]),
            technology_tags: tag_map(),
            web_project_types: set(&["Node.js", "Next.js", "Angular", "Vue", "Vite", "React"]),
            desktop_project_types: set(&[".NET", ".NET Solution", "F#", "VB.NET"]),
            mobile_project_types: set(&["Flutter", "iOS", "Swift", "Android"]),
            cloud_project_types: set(&["Docker"]),
            web_project_files: set(&[
                "next.config.js",
                "next.config.mjs",
                "next.config.ts",
                "angular.json",
                "vue.config.js",
                "vite.config.js",
                "vite.config.ts",
                "nuxt.config.js",
                "nuxt.config.ts",
                "gatsby-config.js",
                "svelte.config.js",
            ]),
            desktop_project_files: set(&[
                "electron-builder.yml",
                "electron-builder.json",
                "tauri.conf.json",
            ]),
            mobile_project_files: set(&["pubspec.yaml", "Podfile", "AndroidManifest.xml"]),
            cloud_project_files: set(&[
                "Dockerfile",
                "docker-compose.yml",
                "docker-compose.yaml",
                "serverless.yml",
                "serverless.yaml",
                "terraform.tf",
                "main.tf",
                "cloudformation.yaml",
                "cloudformation.yml",
                "kubernetes.yaml",
                "k8s.yaml",
            ]),
        }
    }

    pub async fn scan_all_drives(
        &self,
        progress_tx: Option<mpsc::Sender<ScanProgress>>,
    ) -> Result<Vec<ProjectInfo>> {
        let roots = get_available_roots()?;
        self.scan_roots(roots, progress_tx).await
    }

    pub async fn scan_path(
        &self,
        path: &str,
        progress_tx: Option<mpsc::Sender<ScanProgress>>,
    ) -> Result<Vec<ProjectInfo>> {
        self.scan_roots(vec![path.to_string()], progress_tx).await
    }

    pub fn sanitize_projects(&self, projects: Vec<ProjectInfo>) -> Vec<ProjectInfo> {
        self.deduplicate_projects(projects)
    }

    async fn scan_roots(
        &self,
        roots: Vec<String>,
        progress_tx: Option<mpsc::Sender<ScanProgress>>,
    ) -> Result<Vec<ProjectInfo>> {
        let mut projects = Vec::new();
        let total_drives = roots.len() as u32;
        let directories_scanned = AtomicU64::new(0);
        let mut processed_drives = 0u32;

        for root in roots {
            if let Some(tx) = &progress_tx {
                let _ = tx
                    .send(ScanProgress {
                        is_scanning: true,
                        current_drive: root.clone(),
                        current_path: root.clone(),
                        total_drives,
                        processed_drives,
                        projects_found: projects.len() as u32,
                        directories_scanned: directories_scanned.load(Ordering::Relaxed),
                        progress_percentage: percent(processed_drives, total_drives),
                        status: format!("Scanning {root}..."),
                    })
                    .await;
            }

            info!("Scanning root: {root}");
            let mut root_projects = self.scan_root(&root, &directories_scanned).await?;
            projects.append(&mut root_projects);
            processed_drives += 1;
        }

        let projects = self.deduplicate_projects(projects);
        if let Some(tx) = &progress_tx {
            let _ = tx
                .send(ScanProgress {
                    is_scanning: false,
                    current_drive: String::new(),
                    current_path: String::new(),
                    total_drives,
                    processed_drives,
                    projects_found: projects.len() as u32,
                    directories_scanned: directories_scanned.load(Ordering::Relaxed),
                    progress_percentage: 100.0,
                    status: format!("Scan complete. Found {} projects.", projects.len()),
                })
                .await;
        }

        Ok(projects)
    }

    async fn scan_root(
        &self,
        root: &str,
        directories_scanned: &AtomicU64,
    ) -> Result<Vec<ProjectInfo>> {
        let mut projects = Vec::new();
        let mut found_project_roots = HashSet::<String>::new();

        let walker = WalkDir::new(root)
            .max_depth(MAX_SCAN_DEPTH)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| self.allow_entry(root, entry));

        for entry in walker.filter_map(|entry| entry.ok()) {
            if !entry.file_type().is_dir() {
                continue;
            }

            directories_scanned.fetch_add(1, Ordering::Relaxed);
            let path = entry.path();
            if self.should_skip_path(root, path) {
                continue;
            }

            if let Some(project) = self.check_for_project(path) {
                let normalized = normalize_path(&project.path);
                if found_project_roots.insert(normalized) {
                    projects.push(project);
                }
            }
        }

        Ok(self.deduplicate_projects(projects))
    }

    fn allow_entry(&self, scan_root: &str, entry: &DirEntry) -> bool {
        let path = entry.path();
        if self.should_skip_path(scan_root, path) {
            return false;
        }

        match path.file_name() {
            Some(name) => {
                let lowered = name.to_string_lossy().to_lowercase();
                
                // Special handling for mobile project subdirectories
                if self.is_mobile_project_subdirectory(&lowered) {
                    // Allow if not a child of a mobile project, skip otherwise
                    return !self.is_child_of_mobile_project(path);
                }
                
                !self.skip_directories.contains(&lowered)
                    && !lowered.starts_with('.')
                    && !self.should_skip_unix_root_directory(scan_root, path, &lowered)
            }
            None => true,
        }
    }

    fn should_skip_path(&self, scan_root: &str, path: &Path) -> bool {
        if let Some(name) = path.file_name() {
            let lowered = name.to_string_lossy().to_lowercase();
            
            // Check if this is a mobile project subdirectory
            if self.is_mobile_project_subdirectory(&lowered) {
                // Skip if the parent directory is a mobile project (Flutter/React Native)
                if self.is_child_of_mobile_project(path) {
                    return true;
                }
            }
            
            if (path.is_dir() && is_skippable_bundle_name(&lowered))
                || self.skip_directories.contains(&lowered)
                || lowered.starts_with('.')
                || self.should_skip_unix_root_directory(scan_root, path, &lowered)
            {
                return true;
            }
        }

        let normalized_path = normalize_path(&path.to_string_lossy());
        self.exclude_paths.iter().any(|exclude| {
            let normalized_exclude = normalize_path(exclude);
            normalized_path == normalized_exclude
                || normalized_path.starts_with(&format!("{normalized_exclude}\\"))
        })
    }

    fn should_skip_unix_root_directory(
        &self,
        scan_root: &str,
        path: &Path,
        lowered_name: &str,
    ) -> bool {
        self.skip_unix_root_directories.contains(lowered_name)
            && is_unix_filesystem_root(Path::new(scan_root))
            && path.parent() == Some(Path::new(scan_root))
    }

    /// Check if a directory name is a mobile project subdirectory that should be skipped
    fn is_mobile_project_subdirectory(&self, name: &str) -> bool {
        matches!(name, "android" | "ios" | "app" | "pods")
    }

    /// Check if a path is a child of a mobile project (Flutter/React Native)
    fn is_child_of_mobile_project(&self, path: &Path) -> bool {
        // Get the parent directory
        if let Some(parent) = path.parent() {
            // Check if the parent directory contains mobile project markers
            if let Ok(entries) = fs::read_dir(parent) {
                for entry in entries.flatten() {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_file() {
                            let file_name = entry.file_name().to_string_lossy().to_lowercase();
                            // Check for Flutter or React Native project markers
                            if file_name == "pubspec.yaml" || file_name == "package.json" {
                                // For package.json, we need to verify it's actually a React Native project
                                if file_name == "package.json" {
                                    if let Ok(content) = fs::read_to_string(entry.path()) {
                                        if content.to_lowercase().contains("react-native") {
                                            return true;
                                        }
                                    }
                                } else {
                                    // pubspec.yaml definitely indicates a Flutter project
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    fn check_for_project(&self, path: &Path) -> Option<ProjectInfo> {
        if !path.is_dir() {
            return None;
        }

        let entries = match fs::read_dir(path) {
            Ok(entries) => entries.filter_map(|entry| entry.ok()).collect::<Vec<_>>(),
            Err(_) => return None,
        };

        let files = entries
            .iter()
            .filter(|entry| {
                entry
                    .file_type()
                    .map(|kind| kind.is_file())
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        let dirs = entries
            .iter()
            .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
            .collect::<Vec<_>>();
        let file_names = files
            .iter()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();

        for file in &files {
            let file_name = file.file_name().to_string_lossy().to_string();
            let extension = Path::new(&file_name)
                .extension()
                .map(|ext| format!(".{}", ext.to_string_lossy()));

            if let Some(extension) = extension {
                if self.extension_markers.contains(&extension) {
                    return Some(self.create_project_info(
                        path,
                        &file_names,
                        &dirs,
                        &extension,
                        true,
                    ));
                }
            }
        }

        for name in &file_names {
            if self.filename_markers.contains(name) {
                return Some(self.create_project_info(path, &file_names, &dirs, name, false));
            }
        }

        for dir in &dirs {
            let name = dir.file_name().to_string_lossy().to_string();
            if name.ends_with(".xcodeproj") || name.ends_with(".xcworkspace") {
                return Some(self.create_project_info(path, &file_names, &dirs, &name, false));
            }
        }

        let py_files = file_names
            .iter()
            .filter(|name| name.ends_with(".py"))
            .collect::<Vec<_>>();
        if py_files.len() >= 2 {
            let has_main_file = py_files.iter().any(|name| {
                matches!(
                    name.as_str(),
                    "main.py" | "app.py" | "__init__.py" | "__main__.py"
                )
            });
            if has_main_file || py_files.len() >= 3 {
                return Some(self.create_project_info(path, &file_names, &dirs, "main.py", false));
            }
        }

        let has_git = dirs.iter().any(|dir| {
            dir.file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(".git")
        });
        let has_readme = file_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case("README.md"));
        if has_git && has_readme {
            return Some(self.create_project_info(path, &file_names, &dirs, ".git", false));
        }

        None
    }

    fn create_project_info(
        &self,
        path: &Path,
        files: &[String],
        dirs: &[&std::fs::DirEntry],
        matched_marker: &str,
        is_extension_match: bool,
    ) -> ProjectInfo {
        let project_type = if is_extension_match {
            self.project_type_by_extension
                .get(matched_marker)
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string())
        } else if matched_marker.ends_with(".xcodeproj") {
            "iOS".to_string()
        } else if matched_marker.ends_with(".xcworkspace") {
            "macOS".to_string()
        } else {
            self.project_type_by_filename
                .get(matched_marker)
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string())
        };

        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let mut tags = Vec::new();
        if is_extension_match && matches!(matched_marker, ".csproj" | ".fsproj" | ".vbproj") {
            tags.push("C# .NET Core".to_string());
        }

        for file_name in files {
            if let Some(file_tags) = self.technology_tags.get(file_name) {
                tags.extend(file_tags.iter().cloned());
            }
        }

        tags.sort();
        tags.dedup();
        if tags.len() > 4 {
            tags.truncate(4);
        }

        let last_modified = fs::metadata(path)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .map(datetime_from_system_time)
            .unwrap_or_else(Utc::now);
        let is_active = Utc::now().signed_duration_since(last_modified).num_days() <= 7;
        let git_branch = if dirs.iter().any(|dir| {
            dir.file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(".git")
        }) {
            self.read_git_branch(path).ok()
        } else {
            None
        };
        let category = self.detect_category(&project_type, &tags, files, path);
        let (icon_text, icon_color) = get_project_icon(&project_type, &tags);

        let mut project = ProjectInfo::new(path.to_string_lossy().to_string(), name, project_type);
        project.category = category;
        project.tags = tags;
        project.last_modified = last_modified;
        project.is_active = is_active;
        project.git_branch = git_branch;
        project.icon_text = Some(icon_text);
        project.icon_color = Some(icon_color);
        project
    }

    fn detect_category(
        &self,
        project_type: &str,
        tags: &[String],
        files: &[String],
        project_root: &Path,
    ) -> String {
        if self.web_project_types.contains(project_type) {
            return "Web".to_string();
        }
        if self.desktop_project_types.contains(project_type) {
            return "Desktop".to_string();
        }
        if self.mobile_project_types.contains(project_type) {
            return "Mobile".to_string();
        }
        if self.cloud_project_types.contains(project_type) {
            return "Cloud".to_string();
        }

        if project_type.eq_ignore_ascii_case("Python")
            || tags.iter().any(|tag| tag.eq_ignore_ascii_case("Python"))
        {
            if files.iter().any(|file| {
                matches!(
                    file.to_lowercase().as_str(),
                    "manage.py" | "wsgi.py" | "asgi.py"
                )
            }) {
                return "Web".to_string();
            }

            let requirements_path = project_root.join("requirements.txt");
            if let Ok(content) = fs::read_to_string(requirements_path) {
                let lowered = content.to_lowercase();
                if [
                    "django",
                    "flask",
                    "fastapi",
                    "streamlit",
                    "tornado",
                    "bottle",
                ]
                .iter()
                .any(|framework| lowered.contains(framework))
                {
                    return "Web".to_string();
                }
            }
        }

        for tag in tags {
            if self.web_project_types.contains(tag) {
                return "Web".to_string();
            }
            if self.desktop_project_types.contains(tag) {
                return "Desktop".to_string();
            }
            if self.mobile_project_types.contains(tag) {
                return "Mobile".to_string();
            }
            if self.cloud_project_types.contains(tag) {
                return "Cloud".to_string();
            }
        }

        for file in files {
            if self.web_project_files.contains(file) {
                return "Web".to_string();
            }
            if self.desktop_project_files.contains(file) {
                return "Desktop".to_string();
            }
            if self.mobile_project_files.contains(file) {
                return "Mobile".to_string();
            }
            if self.cloud_project_files.contains(file) {
                return "Cloud".to_string();
            }
        }

        if tags.iter().any(|tag| {
            tag.eq_ignore_ascii_case("JavaScript")
                || tag.eq_ignore_ascii_case("Node.js")
                || tag.eq_ignore_ascii_case("TypeScript")
        }) {
            return "Web".to_string();
        }

        "Other".to_string()
    }

    fn deduplicate_projects(&self, projects: Vec<ProjectInfo>) -> Vec<ProjectInfo> {
        let mut unique_by_path = HashMap::<String, ProjectInfo>::new();
        for project in projects {
            if is_path_inside_skippable_location(&project.path) {
                continue;
            }

            let key = normalize_path(&project.path);
            match unique_by_path.get(&key) {
                Some(existing) if project_priority(existing) >= project_priority(&project) => {}
                _ => {
                    unique_by_path.insert(key, project);
                }
            }
        }

        let mut projects = unique_by_path.into_values().collect::<Vec<_>>();
        projects.sort_by(|left, right| {
            path_depth(&left.path)
                .cmp(&path_depth(&right.path))
                .then_with(|| project_priority(right).cmp(&project_priority(left)))
                .then_with(|| left.path.cmp(&right.path))
        });

        let mut filtered = Vec::new();
        for project in projects {
            if filtered
                .iter()
                .any(|existing| should_skip_nested_project(existing, &project))
            {
                continue;
            }
            filtered.push(project);
        }

        filtered
    }

    fn read_git_branch(&self, path: &Path) -> Result<String> {
        let content = fs::read_to_string(path.join(".git").join("HEAD"))?;
        let trimmed = content.trim();

        if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
            Ok(branch.to_string())
        } else {
            Ok(trimmed.chars().take(7).collect())
        }
    }
}

impl Default for ProjectScanner {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

fn tag_map() -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    map.insert(
        "package.json".to_string(),
        vec!["JavaScript".to_string(), "Node.js".to_string()],
    );
    map.insert("tsconfig.json".to_string(), vec!["TypeScript".to_string()]);
    map.insert(
        "next.config.js".to_string(),
        vec!["React".to_string(), "Next.js".to_string()],
    );
    map.insert(
        "next.config.mjs".to_string(),
        vec!["React".to_string(), "Next.js".to_string()],
    );
    map.insert(
        "next.config.ts".to_string(),
        vec!["React".to_string(), "Next.js".to_string()],
    );
    map.insert(
        "angular.json".to_string(),
        vec!["Angular".to_string(), "TypeScript".to_string()],
    );
    map.insert("vue.config.js".to_string(), vec!["Vue".to_string()]);
    map.insert("vite.config.js".to_string(), vec!["Vite".to_string()]);
    map.insert(
        "vite.config.ts".to_string(),
        vec!["Vite".to_string(), "TypeScript".to_string()],
    );
    map.insert("go.mod".to_string(), vec!["Go".to_string()]);
    map.insert("Cargo.toml".to_string(), vec!["Rust".to_string()]);
    map.insert(
        "pom.xml".to_string(),
        vec!["Java".to_string(), "Maven".to_string()],
    );
    map.insert(
        "build.gradle".to_string(),
        vec!["Java".to_string(), "Gradle".to_string()],
    );
    map.insert(
        "build.gradle.kts".to_string(),
        vec![
            "Java".to_string(),
            "Gradle".to_string(),
            "Kotlin".to_string(),
        ],
    );
    map.insert("composer.json".to_string(), vec!["PHP".to_string()]);
    map.insert("requirements.txt".to_string(), vec!["Python".to_string()]);
    map.insert("pyproject.toml".to_string(), vec!["Python".to_string()]);
    map.insert("Pipfile".to_string(), vec!["Python".to_string()]);
    map.insert("setup.py".to_string(), vec!["Python".to_string()]);
    map.insert("main.py".to_string(), vec!["Python".to_string()]);
    map.insert("app.py".to_string(), vec!["Python".to_string()]);
    map.insert("Gemfile".to_string(), vec!["Ruby".to_string()]);
    map.insert(
        "pubspec.yaml".to_string(),
        vec!["Flutter".to_string(), "Dart".to_string()],
    );
    map.insert(
        "Podfile".to_string(),
        vec!["iOS".to_string(), "CocoaPods".to_string()],
    );
    map.insert("Dockerfile".to_string(), vec!["Docker".to_string()]);
    map.insert(
        "docker-compose.yml".to_string(),
        vec!["Docker".to_string(), "Compose".to_string()],
    );
    map.insert(
        "docker-compose.yaml".to_string(),
        vec!["Docker".to_string(), "Compose".to_string()],
    );
    map
}

fn set(items: &[&str]) -> HashSet<String> {
    items.iter().map(|item| item.to_string()).collect()
}

fn map(items: &[(&str, &str)]) -> HashMap<String, String> {
    items
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn percent(processed: u32, total: u32) -> f32 {
    if total == 0 {
        100.0
    } else {
        (processed as f32 / total as f32) * 100.0
    }
}

fn default_exclude_paths() -> Vec<String> {
    #[cfg(windows)]
    let mut paths = Vec::new();

    #[cfg(not(windows))]
    let paths = Vec::new();

    #[cfg(windows)]
    {
        for env_var in ["APPDATA", "LOCALAPPDATA", "TEMP", "TMP"] {
            if let Ok(value) = std::env::var(env_var) {
                if !value.trim().is_empty() {
                    paths.push(value);
                }
            }
        }

        if let Ok(user_profile) = std::env::var("USERPROFILE") {
            if !user_profile.trim().is_empty() {
                paths.push(format!(r"{}\AppData", user_profile));
                paths.push(format!(r"{}\AppData\Local", user_profile));
                paths.push(format!(r"{}\AppData\LocalLow", user_profile));
                paths.push(format!(r"{}\AppData\Roaming", user_profile));
            }
        }
    }

    normalize_exclude_paths(paths)
}

fn normalize_exclude_paths(paths: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();

    for path in paths {
        let candidate = normalize_path(&path);
        if !candidate.is_empty() && seen.insert(candidate.clone()) {
            normalized.push(candidate);
        }
    }

    normalized
}

fn normalize_path(path: &str) -> String {
    path.replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn path_depth(path: &str) -> usize {
    normalize_path(path)
        .split('\\')
        .filter(|segment| !segment.is_empty())
        .count()
}

fn project_priority(project: &ProjectInfo) -> u8 {
    match project.project_type.as_str() {
        ".NET Solution" => 4,
        ".NET" | "F#" | "VB.NET" => 3,
        "Rust" | "Go" | "Node.js" | "Python" | "Flutter" => 2,
        _ => 1,
    }
}

fn is_app_bundle_name(name: &str) -> bool {
    name.ends_with(".app")
}

fn is_skippable_scan_directory_name(name: &str) -> bool {
    matches!(name, "library" | "applications" | "xcode" | "deriveddata")
}

fn is_xcode_bundle_name(name: &str) -> bool {
    name.ends_with(".xcodeproj") || name.ends_with(".xcworkspace")
}

fn is_skippable_bundle_name(name: &str) -> bool {
    is_app_bundle_name(name) || is_xcode_bundle_name(name)
}

fn is_skippable_path_component(name: &str) -> bool {
    is_skippable_scan_directory_name(name) || is_skippable_bundle_name(name)
}

fn is_path_inside_skippable_location(path: &str) -> bool {
    Path::new(path)
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .any(|component| is_skippable_path_component(&component.to_lowercase()))
}

fn should_skip_nested_project(existing: &ProjectInfo, candidate: &ProjectInfo) -> bool {
    existing.project_type == ".NET Solution"
        && matches!(candidate.project_type.as_str(), ".NET" | "F#" | "VB.NET")
        && is_descendant_path(&existing.path, &candidate.path)
}

fn is_descendant_path(parent: &str, child: &str) -> bool {
    let parent_path = Path::new(parent);
    let child_path = Path::new(child);
    child_path != parent_path && child_path.starts_with(parent_path)
}

fn datetime_from_system_time(time: std::time::SystemTime) -> DateTime<Utc> {
    DateTime::<Utc>::from(time)
}

fn is_unix_filesystem_root(path: &Path) -> bool {
    let mut components = path.components();
    matches!(components.next(), Some(Component::RootDir)) && components.next().is_none()
}

#[cfg(windows)]
fn get_available_roots() -> Result<Vec<String>> {
    let mut drives = Vec::new();
    for letter in b'A'..=b'Z' {
        let drive = format!("{}:\\", letter as char);
        if PathBuf::from(&drive).exists() {
            drives.push(drive);
        }
    }
    Ok(drives)
}

#[cfg(target_os = "macos")]
fn get_available_roots() -> Result<Vec<String>> {
    let mut roots = vec!["/".to_string()];
    roots.extend(list_directory_roots(Path::new("/Volumes")));
    roots.sort();
    roots.dedup();
    Ok(roots)
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn get_available_roots() -> Result<Vec<String>> {
    Ok(vec!["/".to_string()])
}

#[cfg(any(target_os = "macos", test))]
fn list_directory_roots(root: &Path) -> Vec<String> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut roots = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            entry
                .file_type()
                .ok()
                .filter(|kind| kind.is_dir())
                .map(|_| path.to_string_lossy().to_string())
        })
        .collect::<Vec<_>>();

    roots.sort();
    roots.dedup();
    roots
}

fn get_project_icon(project_type: &str, tags: &[String]) -> (String, String) {
    if contains_tag(tags, "React") || contains_tag(tags, "Next.js") {
        return ("</>".to_string(), "#2563EB".to_string());
    }
    if contains_tag(tags, "Vue") {
        return ("V".to_string(), "#42B883".to_string());
    }
    if contains_tag(tags, "Angular") {
        return ("A".to_string(), "#DD0031".to_string());
    }
    if contains_tag(tags, "TypeScript") {
        return ("TS".to_string(), "#3178C6".to_string());
    }
    if contains_tag(tags, "Python") {
        return ("Py".to_string(), "#3776AB".to_string());
    }
    if contains_tag(tags, "Go") {
        return ("Go".to_string(), "#00ADD8".to_string());
    }
    if contains_tag(tags, "Rust") {
        return ("Rs".to_string(), "#DEA584".to_string());
    }
    if contains_tag(tags, "Java") {
        return ("Jv".to_string(), "#ED8B00".to_string());
    }
    if contains_tag(tags, "PHP") {
        return ("PHP".to_string(), "#777BB4".to_string());
    }
    if contains_tag(tags, "Ruby") {
        return ("Rb".to_string(), "#CC342D".to_string());
    }
    if contains_tag(tags, "Flutter") || contains_tag(tags, "Dart") {
        return ("Fl".to_string(), "#02569B".to_string());
    }
    if contains_tag(tags, "iOS") || contains_tag(tags, "Swift") {
        return ("iS".to_string(), "#FA7343".to_string());
    }
    if contains_tag(tags, "Docker") {
        return ("Dk".to_string(), "#2496ED".to_string());
    }
    if project_type.contains(".NET") || contains_tag(tags, "C#") {
        return ("C#".to_string(), "#512BD4".to_string());
    }
    if contains_tag(tags, "JavaScript") || contains_tag(tags, "Node.js") {
        return ("JS".to_string(), "#F7DF1E".to_string());
    }

    ("P".to_string(), "#6B7280".to_string())
}

fn contains_tag(tags: &[String], needle: &str) -> bool {
    tags.iter().any(|tag| tag.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::{is_unix_filesystem_root, list_directory_roots, ProjectScanner};
    use crate::models::ProjectInfo;
    use anyhow::Result;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_root(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(format!("{prefix}_{unique}"))
    }

    #[tokio::test]
    async fn detects_rust_project() -> Result<()> {
        let root = unique_root("devatlas_rust_project");
        fs::create_dir_all(&root)?;
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"sample\"\n")?;

        let scanner = ProjectScanner::new(Vec::new());
        let projects = scanner.scan_path(&root.to_string_lossy(), None).await?;

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].project_type, "Rust");

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn prefers_solution_over_nested_dotnet_project() -> Result<()> {
        let root = unique_root("devatlas_solution_project");
        let nested = root.join("App");
        fs::create_dir_all(&nested)?;
        fs::write(root.join("DevAtlas.slnx"), "solution")?;
        fs::write(nested.join("App.csproj"), "<Project />")?;

        let scanner = ProjectScanner::new(Vec::new());
        let projects = scanner.scan_path(&root.to_string_lossy(), None).await?;

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].project_type, ".NET Solution");

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn skips_xcode_bundle_directories_during_scan() -> Result<()> {
        let root = unique_root("devatlas_xcode_project");
        let project_root = root.join("AiFalApp");
        let xcodeproj = project_root.join("AiFalApp.xcodeproj");
        fs::create_dir_all(xcodeproj.join("project.xcworkspace"))?;
        fs::write(xcodeproj.join("project.pbxproj"), "// !PBXProject")?;

        let scanner = ProjectScanner::new(Vec::new());
        let projects = scanner.scan_path(&root.to_string_lossy(), None).await?;

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].path, project_root.to_string_lossy());
        assert_eq!(projects[0].project_type, "iOS");

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn sanitizes_cached_xcode_bundle_entries() {
        let scanner = ProjectScanner::new(Vec::new());
        let root = ProjectInfo::new(
            "/tmp/AiFalApp".to_string(),
            "AiFalApp".to_string(),
            "iOS".to_string(),
        );
        let bundle = ProjectInfo::new(
            "/tmp/AiFalApp/AiFalApp.xcodeproj".to_string(),
            "AiFalApp.xcodeproj".to_string(),
            "macOS".to_string(),
        );

        let projects = scanner.sanitize_projects(vec![bundle, root]);

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].path, "/tmp/AiFalApp");
    }

    #[tokio::test]
    async fn skips_app_bundle_directories_during_scan() -> Result<()> {
        let root = unique_root("devatlas_app_bundle");
        let app_bundle = root.join("Azure Data Studio.app");
        let nested_project = app_bundle.join("Contents").join("Resources").join("app");
        fs::create_dir_all(&nested_project)?;
        fs::write(
            nested_project.join("package.json"),
            "{ \"name\": \"embedded-app\" }",
        )?;

        let scanner = ProjectScanner::new(Vec::new());
        let projects = scanner.scan_path(&root.to_string_lossy(), None).await?;

        assert!(projects.is_empty());

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn sanitizes_cached_app_bundle_entries() {
        let scanner = ProjectScanner::new(Vec::new());
        let bundled = ProjectInfo::new(
            "/Applications/Azure Data Studio.app/Contents/Resources/app".to_string(),
            "app".to_string(),
            "Node.js".to_string(),
        );
        let real = ProjectInfo::new(
            "/tmp/real-project".to_string(),
            "real-project".to_string(),
            "Node.js".to_string(),
        );

        let projects = scanner.sanitize_projects(vec![bundled, real]);

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].path, "/tmp/real-project");
    }

    #[tokio::test]
    async fn skips_library_applications_xcode_and_deriveddata_directories_during_scan() -> Result<()>
    {
        let root = unique_root("devatlas_skipped_dirs");

        let library_project = root.join("Library").join("HistoryProject");
        fs::create_dir_all(&library_project)?;
        fs::write(
            library_project.join("package.json"),
            "{ \"name\": \"history-project\" }",
        )?;

        let applications_project = root.join("Applications").join("EmbeddedWebApp");
        fs::create_dir_all(&applications_project)?;
        fs::write(
            applications_project.join("package.json"),
            "{ \"name\": \"embedded-web-app\" }",
        )?;

        let xcode_project = root.join("Xcode").join("SampleSwift");
        fs::create_dir_all(&xcode_project)?;
        fs::write(
            xcode_project.join("Package.swift"),
            "// swift-tools-version: 5.9",
        )?;

        let derived_data_project = root
            .join("Developer")
            .join("Xcode")
            .join("DerivedData")
            .join("CachedApp");
        fs::create_dir_all(&derived_data_project)?;
        fs::write(
            derived_data_project.join("package.json"),
            "{ \"name\": \"cached-app\" }",
        )?;

        let scanner = ProjectScanner::new(Vec::new());
        let projects = scanner.scan_path(&root.to_string_lossy(), None).await?;

        assert!(projects.is_empty());

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn sanitizes_cached_entries_inside_skipped_directories() {
        let scanner = ProjectScanner::new(Vec::new());
        let library = ProjectInfo::new(
            "/Users/aygundev/Library/Application Support/Cursor/User/History/6eba3ab6".to_string(),
            "6eba3ab6".to_string(),
            "Python".to_string(),
        );
        let applications = ProjectInfo::new(
            "/Volumes/SSD-AYGUN/Applications/SomeTool".to_string(),
            "SomeTool".to_string(),
            "Node.js".to_string(),
        );
        let derived_data = ProjectInfo::new(
            "/Volumes/SSD-AYGUN/Developer/Xcode/DerivedData/CachedApp".to_string(),
            "CachedApp".to_string(),
            "Swift".to_string(),
        );
        let real = ProjectInfo::new(
            "/tmp/real-project".to_string(),
            "real-project".to_string(),
            "Node.js".to_string(),
        );

        let projects = scanner.sanitize_projects(vec![library, applications, derived_data, real]);

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].path, "/tmp/real-project");
    }

    #[test]
    fn identifies_unix_filesystem_root() {
        assert!(is_unix_filesystem_root(Path::new("/")));
        assert!(!is_unix_filesystem_root(Path::new("/Users/dev")));
    }

    #[test]
    fn skips_mac_and_linux_system_dirs_only_from_unix_root() {
        let scanner = ProjectScanner::new(Vec::new());

        assert!(scanner.should_skip_unix_root_directory("/", Path::new("/usr"), "usr"));
        assert!(scanner.should_skip_unix_root_directory("/", Path::new("/Library"), "library"));
        assert!(!scanner.should_skip_unix_root_directory(
            "/Users/dev",
            Path::new("/Users/dev/Library"),
            "library"
        ));
        assert!(!scanner.should_skip_unix_root_directory("/usr", Path::new("/usr/local"), "local"));
    }

    #[test]
    fn lists_only_directory_roots() -> Result<()> {
        let root = unique_root("devatlas_volume_roots");
        let volume_a = root.join("ExternalSSD");
        let volume_b = root.join("Backup");
        fs::create_dir_all(&volume_a)?;
        fs::create_dir_all(&volume_b)?;
        fs::write(root.join("readme.txt"), "ignore me")?;

        let roots = list_directory_roots(&root);

        assert_eq!(roots.len(), 2);
        assert!(roots.iter().any(|path| path.ends_with("ExternalSSD")));
        assert!(roots.iter().any(|path| path.ends_with("Backup")));

        fs::remove_dir_all(root)?;
        Ok(())
    }
}
