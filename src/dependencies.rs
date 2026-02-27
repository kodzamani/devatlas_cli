use crate::models::{DependencyGroup, DependencySection, PackageDependency};
use anyhow::Result;
use regex::Regex;
use reqwest::Client;
use serde_json::Value as JsonValue;
use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use toml::Value as TomlValue;

pub struct DependencyDetector;

impl DependencyDetector {
    pub fn detect(&self, project_path: &str) -> Result<Vec<DependencySection>> {
        let root = Path::new(project_path);
        if !root.is_dir() {
            return Ok(Vec::new());
        }

        let mut sections = Vec::new();
        sections.extend(self.detect_dotnet(root)?);

        if let Some(section) = parse_package_json(&root.join("package.json"))? {
            sections.push(section);
        }
        if let Some(section) = parse_requirements(&root.join("requirements.txt"))? {
            sections.push(section);
        } else if let Some(section) = parse_pyproject(&root.join("pyproject.toml"))? {
            sections.push(section);
        } else if let Some(section) = parse_pipfile(&root.join("Pipfile"))? {
            sections.push(section);
        }
        if let Some(section) = parse_cargo(&root.join("Cargo.toml"))? {
            sections.push(section);
        }
        if let Some(section) = parse_go_mod(&root.join("go.mod"))? {
            sections.push(section);
        }
        if let Some(section) = parse_pubspec(&root.join("pubspec.yaml"))? {
            sections.push(section);
        }
        if let Some(section) = parse_pom(&root.join("pom.xml"))? {
            sections.push(section);
        }
        if let Some(section) = parse_gradle(&root.join("build.gradle"))? {
            sections.push(section);
        } else if let Some(section) = parse_gradle(&root.join("build.gradle.kts"))? {
            sections.push(section);
        }
        if let Some(section) = parse_composer(&root.join("composer.json"))? {
            sections.push(section);
        }
        if let Some(section) = parse_gemfile(&root.join("Gemfile"))? {
            sections.push(section);
        }
        if let Some(section) = parse_swift_dependencies(&root)? {
            sections.push(section);
        }

        Ok(sections)
    }

    fn detect_dotnet(&self, root: &Path) -> Result<Vec<DependencySection>> {
        let mut sections = Vec::new();
        let sln_files = std::fs::read_dir(root)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| matches!(ext, "sln" | "slnx"))
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();

        if let Some(sln) = sln_files.first() {
            let solution_name = sln
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Solution")
                .to_string();
            let groups = walk_matching(root, &["csproj", "fsproj", "vbproj"])
                .into_iter()
                .filter(|path| !contains_build_segment(path))
                .filter_map(|path| parse_csproj(&path).ok().flatten())
                .collect::<Vec<_>>();
            if !groups.is_empty() {
                sections.push(DependencySection {
                    name: solution_name,
                    icon: "⚙️".to_string(),
                    groups,
                });
            }
        } else {
            for path in walk_matching(root, &["csproj", "fsproj", "vbproj"])
                .into_iter()
                .filter(|path| path.parent() == Some(root))
            {
                if let Some(group) = parse_csproj(&path)? {
                    sections.push(DependencySection {
                        name: file_stem_or_name(&path),
                        icon: "⚙️".to_string(),
                        groups: vec![group],
                    });
                }
            }
        }

        Ok(sections)
    }
}

impl Default for DependencyDetector {
    fn default() -> Self {
        Self
    }
}

pub struct PackageUpdateChecker {
    client: Client,
}

impl PackageUpdateChecker {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("DevAtlas CLI/1.0")
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    pub async fn check_sections(&self, sections: &mut [DependencySection]) {
        for section in sections {
            for group in &mut section.groups {
                for package in &mut group.packages {
                    package.is_checking_update = true;
                    package.latest_version = self
                        .latest_version(&package.name, &package.source)
                        .await
                        .ok()
                        .flatten();
                    package.is_checking_update = false;
                }
            }
        }
    }

    async fn latest_version(&self, package_name: &str, source: &str) -> Result<Option<String>> {
        let version = match source {
            "NuGet" => self.nuget_latest(package_name).await,
            "npm" => self.npm_latest(package_name).await,
            "PyPI" => self.pypi_latest(package_name).await,
            "crates.io" => self.crates_latest(package_name).await,
            "pub.dev" => self.pubdev_latest(package_name).await,
            "Maven" => self.maven_latest(package_name).await,
            "Packagist" => self.packagist_latest(package_name).await,
            "RubyGems" => self.rubygems_latest(package_name).await,
            "SwiftPM" => self.swiftpm_latest(package_name).await,
            _ => Ok(None),
        }?;

        Ok(version.filter(|value| !is_prerelease_version(value)))
    }

    async fn nuget_latest(&self, package_name: &str) -> Result<Option<String>> {
        let url = format!(
            "https://api.nuget.org/v3-flatcontainer/{}/index.json",
            package_name.to_lowercase()
        );
        let payload = self.client.get(url).send().await?.text().await?;
        let json: JsonValue = serde_json::from_str(&payload)?;
        Ok(json["versions"].as_array().and_then(|versions| {
            versions.iter().rev().find_map(|value| {
                let version = value.as_str()?;
                (!is_prerelease_version(version)).then(|| version.to_string())
            })
        }))
    }

    async fn npm_latest(&self, package_name: &str) -> Result<Option<String>> {
        let url = format!("https://registry.npmjs.org/{package_name}/latest");
        let payload = self.client.get(url).send().await?.text().await?;
        let json: JsonValue = serde_json::from_str(&payload)?;
        Ok(json["version"].as_str().map(|value| value.to_string()))
    }

    async fn pypi_latest(&self, package_name: &str) -> Result<Option<String>> {
        let url = format!("https://pypi.org/pypi/{package_name}/json");
        let payload = self.client.get(url).send().await?.text().await?;
        let json: JsonValue = serde_json::from_str(&payload)?;
        let stable = json["releases"].as_object().and_then(|releases| {
            releases
                .keys()
                .filter(|version| !is_prerelease_version(version))
                .max_by(|left, right| compare_versions(left, right))
                .cloned()
        });
        Ok(stable.or_else(|| {
            json["info"]["version"]
                .as_str()
                .and_then(|value| (!is_prerelease_version(value)).then(|| value.to_string()))
        }))
    }

    async fn crates_latest(&self, package_name: &str) -> Result<Option<String>> {
        let url = format!("https://crates.io/api/v1/crates/{package_name}");
        let payload = self.client.get(url).send().await?.text().await?;
        let json: JsonValue = serde_json::from_str(&payload)?;
        Ok(json["crate"]["max_stable_version"]
            .as_str()
            .map(|value| value.to_string()))
    }

    async fn pubdev_latest(&self, package_name: &str) -> Result<Option<String>> {
        let url = format!("https://pub.dev/api/packages/{package_name}");
        let payload = self.client.get(url).send().await?.text().await?;
        let json: JsonValue = serde_json::from_str(&payload)?;
        if let Some(version) = json["latest"]["version"].as_str() {
            if !is_prerelease_version(version) {
                return Ok(Some(version.to_string()));
            }
        }

        Ok(json["versions"].as_array().and_then(|versions| {
            versions
                .iter()
                .filter_map(|entry| entry["version"].as_str())
                .filter(|version| !is_prerelease_version(version))
                .max_by(|left, right| compare_versions(left, right))
                .map(|value| value.to_string())
        }))
    }

    async fn maven_latest(&self, package_name: &str) -> Result<Option<String>> {
        let parts = package_name.split(':').collect::<Vec<_>>();
        if parts.len() != 2 {
            return Ok(None);
        }
        let url = format!(
            "https://search.maven.org/solrsearch/select?q=g:{}+AND+a:{}&rows=1&wt=json",
            parts[0], parts[1]
        );
        let payload = self.client.get(url).send().await?.text().await?;
        let json: JsonValue = serde_json::from_str(&payload)?;
        Ok(json["response"]["docs"].as_array().and_then(|docs| {
            docs.iter()
                .filter_map(|doc| doc["latestVersion"].as_str())
                .filter(|version| !is_prerelease_version(version))
                .max_by(|left, right| compare_versions(left, right))
                .map(|value| value.to_string())
        }))
    }

    async fn packagist_latest(&self, package_name: &str) -> Result<Option<String>> {
        let url = format!("https://repo.packagist.org/p2/{package_name}.json");
        let payload = self.client.get(url).send().await?.text().await?;
        let json: JsonValue = serde_json::from_str(&payload)?;
        Ok(json["packages"][package_name]
            .as_array()
            .and_then(|packages| {
                packages.iter().find_map(|package| {
                    let version = package["version"].as_str()?;
                    (!is_prerelease_version(version)).then(|| version.to_string())
                })
            }))
    }

    async fn rubygems_latest(&self, package_name: &str) -> Result<Option<String>> {
        let url = format!("https://rubygems.org/api/v1/versions/{package_name}.json");
        let payload = self.client.get(url).send().await?.text().await?;
        let json: JsonValue = serde_json::from_str(&payload)?;
        Ok(json.as_array().and_then(|versions| {
            versions.iter().find_map(|entry| {
                let version = entry["number"].as_str()?;
                let prerelease = entry["prerelease"].as_bool().unwrap_or(false);
                (!prerelease && !is_prerelease_version(version)).then(|| version.to_string())
            })
        }))
    }

    async fn swiftpm_latest(&self, package_name: &str) -> Result<Option<String>> {
        // For Swift packages, we need to search GitHub since there's no central registry
        // First try to find the repository with exact name match
        let search_url = format!("https://api.github.com/search/repositories?q=\"{}\"+language:Swift&sort=stars&order=desc&per_page=10", package_name);
        
        match self.client.get(&search_url).send().await {
            Ok(response) => {
                let payload = response.text().await?;
                if let Ok(json) = serde_json::from_str::<JsonValue>(&payload) {
                    if let Some(items) = json.get("items").and_then(|i| i.as_array()) {
                        // Try to find the exact match (case-insensitive)
                        for item in items {
                            if let Some(repo_name) = item.get("name").and_then(|n| n.as_str()) {
                                if repo_name.eq_ignore_ascii_case(package_name) {
                                    // Try to get the latest release tag
                                    if let Some(owner) = item.get("owner").and_then(|o| o.get("login")).and_then(|l| l.as_str()) {
                                        let releases_url = format!("https://api.github.com/repos/{}/{}/releases/latest", owner, repo_name);
                                        
                                        if let Ok(releases_response) = self.client.get(&releases_url).send().await {
                                            let releases_payload = releases_response.text().await?;
                                            if let Ok(releases_json) = serde_json::from_str::<JsonValue>(&releases_payload) {
                                                if let Some(tag_name) = releases_json.get("tag_name").and_then(|t| t.as_str()) {
                                                    // Clean up the tag name (remove 'v' prefix if present)
                                                    let version = tag_name.trim_start_matches('v').to_string();
                                                    return Ok(Some(version));
                                                }
                                            }
                                        }
                                        
                                        // If no releases found, fall back to default branch
                                        if let Some(default_branch) = item.get("default_branch").and_then(|b| b.as_str()) {
                                            return Ok(Some(format!("branch:{}", default_branch)));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(_) => {
                // If GitHub API fails, we can't determine the latest version
            }
        }
        
        // As a fallback, we can't determine the latest version without a registry
        Ok(None)
    }
}

impl Default for PackageUpdateChecker {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_csproj(path: &Path) -> Result<Option<DependencyGroup>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let document = roxmltree::Document::parse(&content)?;
    let mut packages = document
        .descendants()
        .filter(|node| node.has_tag_name("PackageReference"))
        .filter_map(|node| {
            let name = node
                .attribute("Include")
                .or_else(|| node.attribute("Update"))
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                return None;
            }
            let version = node
                .attribute("Version")
                .map(|value| value.to_string())
                .or_else(|| {
                    node.children()
                        .find(|child| child.has_tag_name("Version"))
                        .and_then(|child| child.text().map(|value| value.to_string()))
                })
                .unwrap_or_default();
            Some(PackageDependency {
                name,
                version,
                source: "NuGet".to_string(),
                latest_version: None,
                is_checking_update: false,
            })
        })
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| left.name.cmp(&right.name));

    if packages.is_empty() {
        Ok(None)
    } else {
        Ok(Some(DependencyGroup {
            name: file_stem_or_name(path),
            file_path: path.to_string_lossy().to_string(),
            packages,
        }))
    }
}

fn parse_package_json(path: &Path) -> Result<Option<DependencySection>> {
    if !path.exists() {
        return Ok(None);
    }

    let json: JsonValue = serde_json::from_str(&fs::read_to_string(path)?)?;
    let project_name = json["name"]
        .as_str()
        .map(|value| value.to_string())
        .unwrap_or_else(|| directory_name(path));

    let mut section = DependencySection {
        name: project_name,
        icon: "📦".to_string(),
        groups: Vec::new(),
    };
    append_json_dependency_group(path, &json, "dependencies", "npm", &mut section.groups);
    append_json_dependency_group(path, &json, "devDependencies", "npm", &mut section.groups);

    if section.groups.is_empty() {
        Ok(None)
    } else {
        Ok(Some(section))
    }
}

fn parse_requirements(path: &Path) -> Result<Option<DependencySection>> {
    if !path.exists() {
        return Ok(None);
    }

    let requirement_regex = Regex::new(r"^([A-Za-z0-9_\-\.]+)\s*([=~!<>]+)\s*(.+)$")?;
    let name_only_regex = Regex::new(r"^[A-Za-z0-9_\-\.]+$")?;
    let mut packages = Vec::new();

    for line in fs::read_to_string(path)?.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
            continue;
        }
        if let Some(captures) = requirement_regex.captures(trimmed) {
            packages.push(PackageDependency {
                name: captures[1].to_string(),
                version: captures[3].trim().to_string(),
                source: "PyPI".to_string(),
                latest_version: None,
                is_checking_update: false,
            });
        } else if name_only_regex.is_match(trimmed) {
            packages.push(PackageDependency {
                name: trimmed.to_string(),
                version: "*".to_string(),
                source: "PyPI".to_string(),
                latest_version: None,
                is_checking_update: false,
            });
        }
    }

    build_single_group_section(path, "🐍", packages)
}

fn parse_pyproject(path: &Path) -> Result<Option<DependencySection>> {
    if !path.exists() {
        return Ok(None);
    }

    let toml: TomlValue = toml::from_str(&fs::read_to_string(path)?)?;
    let mut packages = Vec::new();

    if let Some(dependencies) = toml
        .get("project")
        .and_then(|project| project.get("dependencies"))
        .and_then(|dependencies| dependencies.as_array())
    {
        for dependency in dependencies {
            if let Some(raw) = dependency.as_str() {
                packages.push(parse_python_dependency(raw));
            }
        }
    }

    if let Some(poetry_dependencies) = toml
        .get("tool")
        .and_then(|tool| tool.get("poetry"))
        .and_then(|poetry| poetry.get("dependencies"))
        .and_then(|dependencies| dependencies.as_table())
    {
        for (name, value) in poetry_dependencies {
            if name.eq_ignore_ascii_case("python") {
                continue;
            }
            packages.push(PackageDependency {
                name: name.clone(),
                version: toml_dependency_version(value),
                source: "PyPI".to_string(),
                latest_version: None,
                is_checking_update: false,
            });
        }
    }

    build_single_group_section(path, "🐍", packages)
}

fn parse_pipfile(path: &Path) -> Result<Option<DependencySection>> {
    if !path.exists() {
        return Ok(None);
    }

    let toml: TomlValue = toml::from_str(&fs::read_to_string(path)?)?;
    let mut packages = Vec::new();
    append_toml_package_table(&mut packages, toml.get("packages"), "PyPI");
    append_toml_package_table(&mut packages, toml.get("dev-packages"), "PyPI");
    build_single_group_section(path, "🐍", packages)
}

fn parse_cargo(path: &Path) -> Result<Option<DependencySection>> {
    if !path.exists() {
        return Ok(None);
    }

    let toml: TomlValue = toml::from_str(&fs::read_to_string(path)?)?;
    let mut packages = Vec::new();
    append_toml_package_table(&mut packages, toml.get("dependencies"), "crates.io");
    append_toml_package_table(&mut packages, toml.get("dev-dependencies"), "crates.io");

    if packages.is_empty() {
        return Ok(None);
    }

    let name = toml
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(TomlValue::as_str)
        .map(|value| value.to_string())
        .unwrap_or_else(|| directory_name(path));
    Ok(Some(DependencySection {
        name,
        icon: "🦀".to_string(),
        groups: vec![DependencyGroup {
            name: "Cargo.toml".to_string(),
            file_path: path.to_string_lossy().to_string(),
            packages: sort_packages(packages),
        }],
    }))
}

fn parse_go_mod(path: &Path) -> Result<Option<DependencySection>> {
    if !path.exists() {
        return Ok(None);
    }

    let require_regex = Regex::new(r"^(\S+)\s+(v[\w\.\-\+]+)")?;
    let mut packages = Vec::new();
    let mut in_require_block = false;
    let content = fs::read_to_string(path)?;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("require (") {
            in_require_block = true;
            continue;
        }
        if in_require_block && trimmed == ")" {
            in_require_block = false;
            continue;
        }
        if !(in_require_block || trimmed.starts_with("require ")) || trimmed.contains("// indirect")
        {
            continue;
        }

        let target = trimmed.trim_start_matches("require").trim();
        if let Some(captures) = require_regex.captures(target) {
            packages.push(PackageDependency {
                name: captures[1].to_string(),
                version: captures[2].to_string(),
                source: "Go Modules".to_string(),
                latest_version: None,
                is_checking_update: false,
            });
        }
    }

    build_single_group_section(path, "🔷", packages)
}

fn parse_pubspec(path: &Path) -> Result<Option<DependencySection>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let mut regular = Vec::new();
    let mut dev = Vec::new();
    let mut current_section = "";
    let mut direct_indent = None::<usize>;

    for line in content.lines() {
        let trimmed_end = line.trim_end();
        let trimmed = trimmed_end.trim_start();
        let indent = trimmed_end.len().saturating_sub(trimmed.len());

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if indent == 0 {
            current_section = if trimmed.starts_with("dependencies:") {
                "dependencies"
            } else if trimmed.starts_with("dev_dependencies:") {
                "dev_dependencies"
            } else {
                ""
            };
            direct_indent = None;
            continue;
        }

        if current_section.is_empty() {
            continue;
        }

        if direct_indent.is_none() {
            direct_indent = Some(indent);
        }
        if Some(indent) != direct_indent {
            continue;
        }

        let Some((name, raw_version)) = trimmed.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if ["sdk", "flutter", "git", "path", "url", "ref", "hosted"].contains(&name) {
            continue;
        }

        let version = raw_version.trim().trim_matches('"').trim_matches('\'');
        let package = PackageDependency {
            name: name.to_string(),
            version: clean_version(version),
            source: "pub.dev".to_string(),
            latest_version: None,
            is_checking_update: false,
        };

        if current_section == "dev_dependencies" {
            dev.push(package);
        } else {
            regular.push(package);
        }
    }

    if regular.is_empty() && dev.is_empty() {
        return Ok(None);
    }

    let mut groups = Vec::new();
    if !regular.is_empty() {
        groups.push(DependencyGroup {
            name: "dependencies".to_string(),
            file_path: path.to_string_lossy().to_string(),
            packages: sort_packages(regular),
        });
    }
    if !dev.is_empty() {
        groups.push(DependencyGroup {
            name: "dev_dependencies".to_string(),
            file_path: path.to_string_lossy().to_string(),
            packages: sort_packages(dev),
        });
    }

    Ok(Some(DependencySection {
        name: directory_name(path),
        icon: "💙".to_string(),
        groups,
    }))
}

fn parse_pom(path: &Path) -> Result<Option<DependencySection>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let document = roxmltree::Document::parse(&content)?;
    let mut packages = Vec::new();
    for dependency in document
        .descendants()
        .filter(|node| node.has_tag_name("dependency"))
    {
        let group_id = child_text(&dependency, "groupId");
        let artifact_id = child_text(&dependency, "artifactId");
        let version = child_text(&dependency, "version");
        if let (Some(group_id), Some(artifact_id)) = (group_id, artifact_id) {
            packages.push(PackageDependency {
                name: format!("{group_id}:{artifact_id}"),
                version: version.unwrap_or_default(),
                source: "Maven".to_string(),
                latest_version: None,
                is_checking_update: false,
            });
        }
    }
    build_single_group_section(path, "☕", packages)
}

fn parse_gradle(path: &Path) -> Result<Option<DependencySection>> {
    if !path.exists() {
        return Ok(None);
    }
    let regex = Regex::new(
        r#"(?:implementation|api|compile|testImplementation|runtimeOnly|compileOnly)\s*\(?\s*['"]([^:]+):([^:]+):([^'"]+)['"]"#,
    )?;
    let content = fs::read_to_string(path)?;
    let packages = regex
        .captures_iter(&content)
        .map(|captures| PackageDependency {
            name: format!("{}:{}", &captures[1], &captures[2]),
            version: captures[3].to_string(),
            source: "Maven".to_string(),
            latest_version: None,
            is_checking_update: false,
        })
        .collect::<Vec<_>>();
    build_single_group_section(path, "☕", packages)
}

fn parse_composer(path: &Path) -> Result<Option<DependencySection>> {
    if !path.exists() {
        return Ok(None);
    }
    let json: JsonValue = serde_json::from_str(&fs::read_to_string(path)?)?;
    let mut section = DependencySection {
        name: json["name"]
            .as_str()
            .map(|value| value.to_string())
            .unwrap_or_else(|| directory_name(path)),
        icon: "🐘".to_string(),
        groups: Vec::new(),
    };
    append_json_dependency_group(path, &json, "require", "Packagist", &mut section.groups);
    append_json_dependency_group(path, &json, "require-dev", "Packagist", &mut section.groups);
    Ok((!section.groups.is_empty()).then_some(section))
}

fn parse_gemfile(path: &Path) -> Result<Option<DependencySection>> {
    if !path.exists() {
        return Ok(None);
    }
    let regex = Regex::new(r#"gem\s+["']([^"']+)["'](?:\s*,\s*["']([^"']+)["'])?"#)?;
    let packages = regex
        .captures_iter(&fs::read_to_string(path)?)
        .map(|captures| PackageDependency {
            name: captures[1].to_string(),
            version: captures
                .get(2)
                .map(|value| value.as_str())
                .unwrap_or("*")
                .to_string(),
            source: "RubyGems".to_string(),
            latest_version: None,
            is_checking_update: false,
        })
        .collect::<Vec<_>>();
    build_single_group_section(path, "💎", packages)
}

fn parse_swift_dependencies(root: &Path) -> Result<Option<DependencySection>> {
    // First try to find and parse Package.resolved files
    if let Some(resolved_path) = find_package_resolved(root) {
        if let Some(section) = parse_package_resolved(&resolved_path)? {
            return Ok(Some(section));
        }
    }
    
    // Fall back to Package.swift parsing
    let package_swift_path = root.join("Package.swift");
    if package_swift_path.exists() {
        if let Some(section) = parse_package_swift(&package_swift_path)? {
            return Ok(Some(section));
        }
    }
    
    Ok(None)
}

fn find_package_resolved(root: &Path) -> Option<PathBuf> {
    // Check for standalone SPM package
    let standalone = root.join("Package.resolved");
    if standalone.exists() {
        return Some(standalone);
    }
    
    // Check inside .xcodeproj or .xcworkspace
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".xcodeproj") || name.ends_with(".xcworkspace") {
                    // Check project.xcworkspace/xcshareddata/swiftpm/Package.resolved
                    let workspace_path = path.join("project.xcworkspace/xcshareddata/swiftpm/Package.resolved");
                    if workspace_path.exists() {
                        return Some(workspace_path);
                    }
                    
                    // Check directly inside workspace for .xcworkspace
                    if name.ends_with(".xcworkspace") {
                        let direct = path.join("xcshareddata/swiftpm/Package.resolved");
                        if direct.exists() {
                            return Some(direct);
                        }
                    }
                }
            }
        }
    }
    
    None
}

fn parse_package_resolved(path: &Path) -> Result<Option<DependencySection>> {
    let content = fs::read_to_string(path)?;
    let json: JsonValue = serde_json::from_str(&content).map_err(|_| anyhow::anyhow!("Failed to parse JSON"))?;
    
    let pins = json.get("pins").and_then(|p| p.as_array()).ok_or_else(|| anyhow::anyhow!("No pins found"))?;
    if pins.is_empty() {
        return Ok(None);
    }
    
    let mut packages = Vec::new();
    for pin in pins {
        // Support both v2/v3 (identity + location) and v1 (package + repositoryURL) formats
        let identity = pin.get("identity").and_then(|i| i.as_str());
        let location = pin.get("location").and_then(|l| l.as_str());
        let repository_url = pin.get("repositoryURL").and_then(|r| r.as_str()); // v1 fallback
        
        let url = location.or(repository_url).unwrap_or("");
        let raw_name = identity.or_else(|| extract_package_name_from_url(url)).unwrap_or("");
        if raw_name.is_empty() {
            continue;
        }
        
        // Capitalize first letter to match common conventions
        let name = if raw_name.is_empty() {
            "SwiftPackage".to_string()
        } else {
            let mut chars = raw_name.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        };
        
        let mut version = "*".to_string();
        if let Some(state) = pin.get("state").and_then(|s| s.as_object()) {
            if let Some(v) = state.get("version").and_then(|v| v.as_str()) {
                version = v.to_string();
            } else if let Some(b) = state.get("branch").and_then(|b| b.as_str()) {
                version = format!("branch:{}", b);
            } else if let Some(r) = state.get("revision").and_then(|r| r.as_str()) {
                version = format!("rev:{}", &r[..7.min(r.len())]);
            }
        }
        
        packages.push(PackageDependency {
            name,
            version,
            source: "SwiftPM".to_string(),
            latest_version: None,
            is_checking_update: false,
        });
    }
    
    build_single_group_section(path, "🍎", packages)
}

fn parse_package_swift(path: &Path) -> Result<Option<DependencySection>> {
    if !path.exists() {
        return Ok(None);
    }
    
    let content = fs::read_to_string(path)?;
    
    // Collapse multi-line declarations by replacing newlines with spaces
    let collapsed = content.replace('\n', " ");
    
    // Match every .package(...) block
    let package_pattern = r#"\.package\s*\([^)]+\)"#;
    let regex = Regex::new(package_pattern)?;
    
    let mut packages = Vec::new();
    for captures in regex.captures_iter(&collapsed) {
        if let Some(block) = captures.get(0) {
            if let Some((name, version)) = parse_package_dependency_block(block.as_str()) {
                packages.push(PackageDependency {
                    name,
                    version,
                    source: "SwiftPM".to_string(),
                    latest_version: None,
                    is_checking_update: false,
                });
            }
        }
    }
    
    build_single_group_section(path, "🍎", packages)
}

fn parse_package_dependency_block(block: &str) -> Option<(String, String)> {
    // Extract URL
    let url_regex = Regex::new(r#"url:\s*"([^"]+)""#).ok()?;
    let url = if let Some(captures) = url_regex.captures(block) {
        captures.get(1)?.as_str()
    } else {
        return None;
    };
    
    let name = extract_package_name_from_url(url).unwrap_or("SwiftPackage");
    
    // Extract version with various patterns
    let patterns = [
        (r#"from:\s*"([^"]+)""#, ""),
        (r#"exact:\s*"([^"]+)""#, ""),
        (r#"branch:\s*"([^"]+)""#, "branch:"),
        (r#"revision:\s*"([^"]+)""#, "rev:"),
    ];
    
    for (pattern, prefix) in patterns {
        if let Ok(regex) = Regex::new(pattern) {
            if let Some(captures) = regex.captures(block) {
                let value = captures.get(1)?.as_str();
                let version = if prefix == "rev:" {
                    format!("{}{}", prefix, &value[..7.min(value.len())])
                } else if prefix.is_empty() {
                    value.to_string()
                } else {
                    format!("{}{}", prefix, value)
                };
                return Some((name.to_string(), version));
            }
        }
    }
    
    Some((name.to_string(), "*".to_string()))
}

fn extract_package_name_from_url(url: &str) -> Option<&str> {
    let components: Vec<&str> = url.split('/').collect();
    if let Some(last) = components.last() {
        let name = last
            .trim_end_matches(".git")
            .trim_end_matches(".package");
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

fn build_single_group_section(
    path: &Path,
    icon: &str,
    packages: Vec<PackageDependency>,
) -> Result<Option<DependencySection>> {
    if packages.is_empty() {
        return Ok(None);
    }
    Ok(Some(DependencySection {
        name: directory_name(path),
        icon: icon.to_string(),
        groups: vec![DependencyGroup {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("dependencies")
                .to_string(),
            file_path: path.to_string_lossy().to_string(),
            packages: sort_packages(packages),
        }],
    }))
}

fn append_json_dependency_group(
    path: &Path,
    json: &JsonValue,
    property: &str,
    source: &str,
    groups: &mut Vec<DependencyGroup>,
) {
    let Some(object) = json.get(property).and_then(JsonValue::as_object) else {
        return;
    };

    let mut packages = object
        .iter()
        .filter(|(name, _)| {
            !(property == "require" && (*name == "php" || name.starts_with("ext-")))
        })
        .map(|(name, value)| PackageDependency {
            name: name.clone(),
            version: clean_version(value.as_str().unwrap_or_default()),
            source: source.to_string(),
            latest_version: None,
            is_checking_update: false,
        })
        .collect::<Vec<_>>();

    if packages.is_empty() {
        return;
    }
    packages.sort_by(|left, right| left.name.cmp(&right.name));
    groups.push(DependencyGroup {
        name: property.to_string(),
        file_path: path.to_string_lossy().to_string(),
        packages,
    });
}

fn append_toml_package_table(
    packages: &mut Vec<PackageDependency>,
    table: Option<&TomlValue>,
    source: &str,
) {
    let Some(table) = table.and_then(TomlValue::as_table) else {
        return;
    };
    for (name, value) in table {
        packages.push(PackageDependency {
            name: name.clone(),
            version: toml_dependency_version(value),
            source: source.to_string(),
            latest_version: None,
            is_checking_update: false,
        });
    }
}

fn parse_python_dependency(raw: &str) -> PackageDependency {
    let regex = Regex::new(r"^([A-Za-z0-9_\-\.]+)\s*([<>=!~]+)?\s*(.*)$")
        .expect("python dependency regex should compile");
    if let Some(captures) = regex.captures(raw) {
        PackageDependency {
            name: captures[1].to_string(),
            version: clean_version(captures.get(3).map(|value| value.as_str()).unwrap_or("*")),
            source: "PyPI".to_string(),
            latest_version: None,
            is_checking_update: false,
        }
    } else {
        PackageDependency {
            name: raw.to_string(),
            version: "*".to_string(),
            source: "PyPI".to_string(),
            latest_version: None,
            is_checking_update: false,
        }
    }
}

fn toml_dependency_version(value: &TomlValue) -> String {
    if let Some(version) = value.as_str() {
        return clean_version(version);
    }
    if let Some(table) = value.as_table() {
        if let Some(version) = table.get("version").and_then(TomlValue::as_str) {
            return clean_version(version);
        }
        if table.contains_key("path") {
            return "path".to_string();
        }
    }
    "*".to_string()
}

fn child_text(node: &roxmltree::Node<'_, '_>, tag_name: &str) -> Option<String> {
    node.children()
        .find(|child| child.has_tag_name(tag_name))
        .and_then(|child| child.text())
        .map(|value| value.to_string())
}

fn walk_matching(root: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
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
                        .any(|candidate| candidate.eq_ignore_ascii_case(ext))
                })
                .unwrap_or(false)
        })
        .collect()
}

fn contains_build_segment(path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy().to_lowercase();
        value == "bin" || value == "obj"
    })
}

fn file_stem_or_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .or_else(|| path.file_name().and_then(|name| name.to_str()))
        .unwrap_or("unknown")
        .to_string()
}

fn directory_name(path: &Path) -> String {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_string()
}

fn sort_packages(mut packages: Vec<PackageDependency>) -> Vec<PackageDependency> {
    packages.sort_by(|left, right| left.name.cmp(&right.name));
    packages
}

fn clean_version(version: &str) -> String {
    version
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_start_matches('^')
        .trim_start_matches('~')
        .to_string()
}

fn is_prerelease_version(version: &str) -> bool {
    let normalized = version.trim().to_ascii_lowercase();
    let prerelease_markers = [
        "-preview", ".preview", " preview", "-pre", ".pre", "-alpha", ".alpha", "-beta", ".beta",
        "-rc", ".rc", "-dev", ".dev", "-canary", ".canary",
    ];

    prerelease_markers
        .iter()
        .any(|marker| normalized.contains(marker))
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let left_parts = tokenize_version(left);
    let right_parts = tokenize_version(right);

    for index in 0..left_parts.len().max(right_parts.len()) {
        let left_part = left_parts
            .get(index)
            .copied()
            .unwrap_or(VersionToken::Number(0));
        let right_part = right_parts
            .get(index)
            .copied()
            .unwrap_or(VersionToken::Number(0));

        match (left_part, right_part) {
            (VersionToken::Number(left_num), VersionToken::Number(right_num)) => {
                let ordering = left_num.cmp(&right_num);
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (VersionToken::Text(left_text), VersionToken::Text(right_text)) => {
                let ordering = left_text.cmp(right_text);
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (VersionToken::Number(_), VersionToken::Text(_)) => return Ordering::Greater,
            (VersionToken::Text(_), VersionToken::Number(_)) => return Ordering::Less,
        }
    }

    Ordering::Equal
}

#[derive(Clone, Copy)]
enum VersionToken<'a> {
    Number(u64),
    Text(&'a str),
}

fn tokenize_version(version: &str) -> Vec<VersionToken<'_>> {
    version
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| match part.parse::<u64>() {
            Ok(number) => VersionToken::Number(number),
            Err(_) => VersionToken::Text(part),
        })
        .collect()
}
