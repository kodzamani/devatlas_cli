use crate::models::UnusedCodeResult;
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

const GLOBAL_SKIP_DIRS: &[&str] = &[
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
];

#[derive(Clone)]
struct Symbol {
    name: String,
    line: usize,
}

impl Symbol {
    fn new(name: &str, line: usize) -> Self {
        Self {
            name: name.to_string(),
            line,
        }
    }
}

trait LanguageAnalyzer {
    fn analyze(&self, project_path: &str) -> Vec<UnusedCodeResult>;
}

#[derive(Default)]
struct AnalyzerService;

impl AnalyzerService {
    fn analyze(&self, project_path: &str) -> Vec<UnusedCodeResult> {
        let findings = match detect_primary_language(project_path).as_str() {
            "C#" => CSharpAnalyzer.analyze(project_path),
            "JavaScript" => JavaScriptAnalyzer.analyze(project_path),
            "Dart" => DartAnalyzer.analyze(project_path),
            "Swift" => SwiftAnalyzer.analyze(project_path),
            _ => CSharpAnalyzer.analyze(project_path),
        };
        dedupe_and_sort(findings)
    }
}

pub struct UnusedCodeAnalyzer;

impl UnusedCodeAnalyzer {
    pub fn analyze(&self, project_path: &str) -> Vec<UnusedCodeResult> {
        AnalyzerService.analyze(project_path)
    }
}

impl Default for UnusedCodeAnalyzer {
    fn default() -> Self {
        Self
    }
}

struct CSharpAnalyzer;

impl LanguageAnalyzer for CSharpAnalyzer {
    fn analyze(&self, project_path: &str) -> Vec<UnusedCodeResult> {
        let files = collect_files(
            project_path,
            &["cs"],
            &["migrations"],
            &["modelsnapshot.cs"],
        );
        let combined = read_combined(&files);
        let skip_types = ["Program", "App", "AssemblyInfo", "String", "Task", "List"];
        let field_re =
            re(r"private\s+(?:readonly\s+)?(?:[\w<>\[\]]+\s+)?([a-zA-Z_][a-zA-Z0-9_]*)\s*;");
        let method_re = re(r"private\s+[\w<>\[\]]+\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(");
        let private_class_re = re(r"private\s+(?:partial\s+)?class\s+([a-zA-Z_][a-zA-Z0-9_]*)");
        let class_re = re(
            r"(?:public|private|internal|protected|internal\s+protected|private\s+protected)?\s*(?:partial\s+)?class\s+([a-zA-Z_][a-zA-Z0-9_]*)",
        );
        let struct_re =
            re(r"(?:public|private|internal|protected)?\s*struct\s+([a-zA-Z_][a-zA-Z0-9_]*)");
        let enum_re =
            re(r"(?:public|private|internal|protected)?\s*enum\s+([a-zA-Z_][a-zA-Z0-9_]*)");
        let interface_re =
            re(r"(?:public|private|internal|protected)?\s*interface\s+([a-zA-Z_][a-zA-Z0-9_]*)");
        let property_re = re(r"private\s+[\w<>\[\]]+\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\{\s*get");
        let local_var_re = re(
            r"(?:var|int|string|bool|double|float|decimal|long|short|byte|char|object)\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*=",
        );
        let mut results = Vec::new();

        for file in files {
            let Ok(content) = fs::read_to_string(&file) else {
                continue;
            };

            let mut fields = Vec::new();
            let mut methods = Vec::new();
            let mut private_classes = Vec::new();
            let mut properties = Vec::new();
            let mut locals = Vec::new();
            let mut classes = Vec::new();
            let mut structs = Vec::new();
            let mut enums = Vec::new();
            let mut interfaces = Vec::new();

            for (index, line) in content.lines().enumerate() {
                let line_number = index + 1;
                let trimmed = line.trim_start();
                if trimmed.starts_with('[')
                    || trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with('*')
                {
                    continue;
                }

                if let Some(name) = capture(&field_re, line) {
                    if !name.starts_with('_') && name.len() > 1 {
                        fields.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&method_re, line) {
                    methods.push(Symbol::new(&name, line_number));
                }
                if let Some(name) = capture(&private_class_re, line) {
                    private_classes.push(Symbol::new(&name, line_number));
                }
                if let Some(name) = capture(&property_re, line) {
                    properties.push(Symbol::new(&name, line_number));
                }
                if let Some(name) = capture(&local_var_re, line) {
                    if !matches!(name.as_str(), "var" | "true" | "false") {
                        locals.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&class_re, line) {
                    if !starts_with_any(&name, &["_"]) && !contains_ignore_case(&skip_types, &name)
                    {
                        classes.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&struct_re, line) {
                    if !starts_with_any(&name, &["_"]) && !contains_ignore_case(&skip_types, &name)
                    {
                        structs.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&enum_re, line) {
                    if !starts_with_any(&name, &["_"]) && !contains_ignore_case(&skip_types, &name)
                    {
                        enums.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&interface_re, line) {
                    if !starts_with_any(&name, &["_"]) && !contains_ignore_case(&skip_types, &name)
                    {
                        interfaces.push(Symbol::new(&name, line_number));
                    }
                }
            }

            for field in &fields {
                if count_word(&content, &field.name) <= 1 {
                    push(
                        &mut results,
                        "field",
                        field,
                        &file,
                        "Private field appears unused",
                    );
                }
            }
            for method in &methods {
                if count_pattern(
                    &content,
                    &format!(r"\b{}\s*\(", regex::escape(&method.name)),
                ) <= 1
                {
                    push(
                        &mut results,
                        "method",
                        method,
                        &file,
                        "Private method appears unused",
                    );
                }
            }
            for class_symbol in &private_classes {
                if count_word(&content, &class_symbol.name) <= 1 {
                    push(
                        &mut results,
                        "class",
                        class_symbol,
                        &file,
                        "Private class appears unused",
                    );
                }
            }
            for property in &properties {
                if count_word(&content, &property.name) <= 1 {
                    push(
                        &mut results,
                        "property",
                        property,
                        &file,
                        "Private property appears unused",
                    );
                }
            }
            for local_var in &locals {
                if count_word(&content, &local_var.name) <= 1 {
                    push(
                        &mut results,
                        "variable",
                        local_var,
                        &file,
                        "Local variable appears unused",
                    );
                }
            }
            for item in &classes {
                if count_word(&combined, &item.name) <= 1 {
                    push(
                        &mut results,
                        "class",
                        item,
                        &file,
                        "Class appears unused across project",
                    );
                }
            }
            for item in &structs {
                if count_word(&combined, &item.name) <= 1 {
                    push(
                        &mut results,
                        "struct",
                        item,
                        &file,
                        "Struct appears unused across project",
                    );
                }
            }
            for item in &enums {
                if count_word(&combined, &item.name) <= 1 {
                    push(
                        &mut results,
                        "enum",
                        item,
                        &file,
                        "Enum appears unused across project",
                    );
                }
            }
            for item in &interfaces {
                if count_word(&combined, &item.name) <= 1 {
                    push(
                        &mut results,
                        "interface",
                        item,
                        &file,
                        "Interface appears unused across project",
                    );
                }
            }
        }

        results
    }
}

struct JavaScriptAnalyzer;

impl LanguageAnalyzer for JavaScriptAnalyzer {
    fn analyze(&self, project_path: &str) -> Vec<UnusedCodeResult> {
        let files = collect_files(
            project_path,
            &["js", "jsx", "ts", "tsx", "mjs", "cjs"],
            &["coverage", ".vs"],
            &[],
        );
        let const_re = re(r"(?:const|let|var)\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*=");
        let function_re = re(r"function\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*\(");
        let arrow_re = re(
            r"(?:const|let|var)\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*=\s*(?:\([^)]*\)|[a-zA-Z_$][a-zA-Z0-9_$]*)\s*=>",
        );
        let class_re = re(r"class\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*(?:extends|<|\{)");
        let interface_re = re(r"interface\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*(?:<|\{)");
        let mut results = Vec::new();

        for file in files {
            let Ok(content) = fs::read_to_string(&file) else {
                continue;
            };

            let mut variables = Vec::new();
            let mut functions = Vec::new();
            let mut arrows = Vec::new();
            let mut classes = Vec::new();
            let mut interfaces = Vec::new();

            for (index, line) in content.lines().enumerate() {
                let line_number = index + 1;
                let trimmed = line.trim_start();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with('*')
                {
                    continue;
                }

                if let Some(name) = capture(&const_re, line) {
                    if !name.starts_with('_') && !matches!(name.as_str(), "undefined" | "NaN") {
                        variables.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&function_re, line) {
                    functions.push(Symbol::new(&name, line_number));
                }
                if let Some(name) = capture(&arrow_re, line) {
                    arrows.push(Symbol::new(&name, line_number));
                }
                if let Some(name) = capture(&class_re, line) {
                    classes.push(Symbol::new(&name, line_number));
                }
                if let Some(name) = capture(&interface_re, line) {
                    interfaces.push(Symbol::new(&name, line_number));
                }
            }

            for item in &variables {
                if count_word(&content, &item.name) <= 1 {
                    push(
                        &mut results,
                        "variable",
                        item,
                        &file,
                        "Variable appears unused",
                    );
                }
            }
            for item in &functions {
                if count_pattern(&content, &format!(r"\b{}\s*\(", regex::escape(&item.name))) <= 1 {
                    push(
                        &mut results,
                        "function",
                        item,
                        &file,
                        "Function appears unused",
                    );
                }
            }
            for item in &arrows {
                if count_word(&content, &item.name) <= 1 {
                    push(
                        &mut results,
                        "arrow function",
                        item,
                        &file,
                        "Arrow function appears unused",
                    );
                }
            }
            for item in &classes {
                if count_word(&content, &item.name) <= 1 {
                    push(&mut results, "class", item, &file, "Class appears unused");
                }
            }
            for item in &interfaces {
                if count_pattern(
                    &content,
                    &format!(
                        r":\s*{}\b|<\s*{}\b",
                        regex::escape(&item.name),
                        regex::escape(&item.name)
                    ),
                ) == 0
                {
                    push(
                        &mut results,
                        "interface",
                        item,
                        &file,
                        "Interface appears unused",
                    );
                }
            }
        }

        results
    }
}

struct DartAnalyzer;

impl LanguageAnalyzer for DartAnalyzer {
    fn analyze(&self, project_path: &str) -> Vec<UnusedCodeResult> {
        let files = collect_files(project_path, &["dart"], &[], &[]);
        let combined = read_combined(&files);
        let builtins = [
            "String",
            "int",
            "double",
            "num",
            "bool",
            "dynamic",
            "void",
            "Widget",
            "BuildContext",
            "StatefulWidget",
            "StatelessWidget",
            "ChangeNotifier",
            "main",
            "runApp",
            "context",
        ];
        let class_re = re(r"(?:abstract\s+)?class\s+([a-zA-Z_][a-zA-Z0-9_]*)");
        let mixin_re = re(r"mixin\s+([a-zA-Z_][a-zA-Z0-9_]*)");
        let extension_re =
            re(r"extension\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+on\s+[a-zA-Z_][a-zA-Z0-9_<>, ?]*");
        let enum_re = re(r"enum\s+([a-zA-Z_][a-zA-Z0-9_]*)");
        let typedef_re =
            re(r"typedef\s+(?:[a-zA-Z_][a-zA-Z0-9_<>,\s]*\s+)?([a-zA-Z_][a-zA-Z0-9_]*)\s*=");
        let top_level_var_re =
            re(r"(?:static\s+)?(?:final|const|var)\s+(?:<[^>]+>\s+)?([a-zA-Z_][a-zA-Z0-9_]*)\s*=");
        let private_method_re = re(
            r"(?:void|int|double|String|bool|dynamic|var|final|Widget|Stream|Future)?\s*(?:\?\s*)?_([a-zA-Z_][a-zA-Z0-9_]*)\s*\(",
        );
        let private_var_re = re(r"(?:final|const|var)\s+_([a-zA-Z_][a-zA-Z0-9_]*)");
        let static_method_re = re(
            r"static\s+(?:void|int|double|String|bool|dynamic|Widget|Stream|Future)?\s*(?:\?\s*)?([a-zA-Z_][a-zA-Z0-9_]*)\s*\(",
        );
        let stateless_re = re(r"class\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+extends\s+StatelessWidget");
        let stateful_re = re(r"class\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+extends\s+StatefulWidget");
        let notifier_re = re(r"class\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+extends\s+ChangeNotifier");
        let mut results = Vec::new();

        for file in files {
            let Ok(content) = fs::read_to_string(&file) else {
                continue;
            };

            let mut classes = Vec::new();
            let mut mixins = Vec::new();
            let mut extensions = Vec::new();
            let mut enums = Vec::new();
            let mut typedefs = Vec::new();
            let mut top_vars = Vec::new();
            let mut private_methods = Vec::new();
            let mut private_vars = Vec::new();
            let mut static_methods = Vec::new();
            let mut stateless_widgets = Vec::new();
            let mut stateful_widgets = Vec::new();
            let mut change_notifiers = Vec::new();

            for (index, line) in content.lines().enumerate() {
                let line_number = index + 1;
                let trimmed = line.trim_start();
                if trimmed.is_empty()
                    || trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with('*')
                    || trimmed.starts_with('@')
                    || trimmed.starts_with("import ")
                    || trimmed.starts_with("export ")
                    || trimmed.starts_with("part ")
                {
                    continue;
                }

                if let Some(name) = capture(&class_re, line) {
                    if !starts_with_any(&name, &["_"]) && !contains_ignore_case(&builtins, &name) {
                        classes.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&mixin_re, line) {
                    if !starts_with_any(&name, &["_"]) && !contains_ignore_case(&builtins, &name) {
                        mixins.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&extension_re, line) {
                    if !contains_ignore_case(&builtins, &name) {
                        extensions.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&enum_re, line) {
                    if !starts_with_any(&name, &["_"]) && !contains_ignore_case(&builtins, &name) {
                        enums.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&typedef_re, line) {
                    if !starts_with_any(&name, &["_"]) && !contains_ignore_case(&builtins, &name) {
                        typedefs.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&top_level_var_re, line) {
                    if !starts_with_any(&name, &["_"]) && !contains_ignore_case(&builtins, &name) {
                        top_vars.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&private_method_re, line) {
                    private_methods.push(Symbol::new(&name, line_number));
                }
                if let Some(name) = capture(&private_var_re, line) {
                    private_vars.push(Symbol::new(&name, line_number));
                }
                if let Some(name) = capture(&static_method_re, line) {
                    static_methods.push(Symbol::new(&name, line_number));
                }
                if let Some(name) = capture(&stateless_re, line) {
                    stateless_widgets.push(Symbol::new(&name, line_number));
                }
                if let Some(name) = capture(&stateful_re, line) {
                    stateful_widgets.push(Symbol::new(&name, line_number));
                }
                if let Some(name) = capture(&notifier_re, line) {
                    change_notifiers.push(Symbol::new(&name, line_number));
                }
            }

            for item in &classes {
                push_cross(
                    &mut results,
                    &combined,
                    &file,
                    item,
                    "class",
                    "Class appears unused across project",
                );
            }
            for item in &mixins {
                if count_pattern(
                    &combined,
                    &format!(r"with\s+{}\b", regex::escape(&item.name)),
                ) == 0
                {
                    push(
                        &mut results,
                        "mixin",
                        item,
                        &file,
                        "Mixin appears unused across project",
                    );
                }
            }
            for item in &extensions {
                push_cross(
                    &mut results,
                    &combined,
                    &file,
                    item,
                    "extension",
                    "Extension appears unused across project",
                );
            }
            for item in &enums {
                push_cross(
                    &mut results,
                    &combined,
                    &file,
                    item,
                    "enum",
                    "Enum appears unused across project",
                );
            }
            for item in &typedefs {
                push_cross(
                    &mut results,
                    &combined,
                    &file,
                    item,
                    "typedef",
                    "Typedef appears unused across project",
                );
            }
            for item in &top_vars {
                push_cross(
                    &mut results,
                    &combined,
                    &file,
                    item,
                    "variable",
                    "Top-level variable appears unused",
                );
            }
            for item in &stateless_widgets {
                push_cross(
                    &mut results,
                    &combined,
                    &file,
                    item,
                    "StatelessWidget",
                    "StatelessWidget appears unused across project",
                );
            }
            for item in &stateful_widgets {
                push_cross(
                    &mut results,
                    &combined,
                    &file,
                    item,
                    "StatefulWidget",
                    "StatefulWidget appears unused across project",
                );
            }
            for item in &change_notifiers {
                push_cross(
                    &mut results,
                    &combined,
                    &file,
                    item,
                    "ChangeNotifier",
                    "ChangeNotifier appears unused across project",
                );
            }
            for item in &private_methods {
                if count_pattern(&content, &format!(r"_?{}\s*\(", regex::escape(&item.name))) <= 1 {
                    push(
                        &mut results,
                        "method",
                        item,
                        &file,
                        "Private method appears unused",
                    );
                }
            }
            for item in &private_vars {
                if count_pattern(&content, &format!(r"_?{}\b", regex::escape(&item.name))) <= 1 {
                    push(
                        &mut results,
                        "variable",
                        item,
                        &file,
                        "Private variable appears unused",
                    );
                }
            }
            for item in &static_methods {
                if count_pattern(&content, &format!(r"\b{}\s*\(", regex::escape(&item.name))) <= 1 {
                    push(
                        &mut results,
                        "static method",
                        item,
                        &file,
                        "Static method appears unused",
                    );
                }
            }
        }

        results.extend(analyze_pubspec(project_path));
        results
    }
}

struct SwiftAnalyzer;

impl LanguageAnalyzer for SwiftAnalyzer {
    fn analyze(&self, project_path: &str) -> Vec<UnusedCodeResult> {
        let files = collect_files(project_path, &["swift"], &[], &[]);
        let combined = read_combined(&files);
        let builtins = ["String", "Int", "Bool", "View", "App", "AppDelegate"];
        let class_re = re(
            r"(?:open|public|internal|fileprivate|private)?\s*(?:final\s+)?class\s+([a-zA-Z_][a-zA-Z0-9_]*)",
        );
        let struct_re = re(
            r"(?:open|public|internal|fileprivate|private)?\s*struct\s+([a-zA-Z_][a-zA-Z0-9_]*)",
        );
        let enum_re =
            re(r"(?:open|public|internal|fileprivate|private)?\s*enum\s+([a-zA-Z_][a-zA-Z0-9_]*)");
        let protocol_re = re(
            r"(?:open|public|internal|fileprivate|private)?\s*protocol\s+([a-zA-Z_][a-zA-Z0-9_]*)",
        );
        let extension_re = re(
            r"(?:open|public|internal|fileprivate|private)?\s*extension\s+([a-zA-Z_][a-zA-Z0-9_]*)",
        );
        let private_var_re = re(r"private\s+(?:var|let)\s+([a-zA-Z0-9_]+)");
        let private_func_re = re(r"private\s+func\s+([a-zA-Z0-9_]+)");
        let mut results = Vec::new();

        for file in files {
            let Ok(content) = fs::read_to_string(&file) else {
                continue;
            };

            let mut classes = Vec::new();
            let mut structs = Vec::new();
            let mut enums = Vec::new();
            let mut protocols = Vec::new();
            let mut extensions = Vec::new();
            let mut private_vars = Vec::new();
            let mut private_funcs = Vec::new();

            for (index, line) in content.lines().enumerate() {
                let line_number = index + 1;
                let trimmed = line.trim_start();
                if trimmed.is_empty()
                    || trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with('*')
                    || trimmed.starts_with('@')
                    || trimmed.starts_with("import ")
                    || trimmed.starts_with("#if")
                    || trimmed.starts_with("#else")
                    || trimmed.starts_with("#endif")
                {
                    continue;
                }

                if let Some(name) = capture(&class_re, line) {
                    if !contains_ignore_case(&builtins, &name)
                        && !starts_with_any(&name, &["_", "UI", "NS"])
                    {
                        classes.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&struct_re, line) {
                    if !contains_ignore_case(&builtins, &name)
                        && !starts_with_any(&name, &["_", "UI", "NS"])
                    {
                        structs.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&enum_re, line) {
                    if !contains_ignore_case(&builtins, &name) && !name.starts_with('_') {
                        enums.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&protocol_re, line) {
                    if !contains_ignore_case(&builtins, &name) && !name.starts_with('_') {
                        protocols.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&extension_re, line) {
                    if !contains_ignore_case(&builtins, &name) && !name.starts_with('_') {
                        extensions.push(Symbol::new(&name, line_number));
                    }
                }
                if let Some(name) = capture(&private_var_re, line) {
                    private_vars.push(Symbol::new(&name, line_number));
                }
                if let Some(name) = capture(&private_func_re, line) {
                    private_funcs.push(Symbol::new(&name, line_number));
                }
            }

            for item in &classes {
                push_cross(
                    &mut results,
                    &combined,
                    &file,
                    item,
                    "class",
                    "Class appears unused across project",
                );
            }
            for item in &structs {
                push_cross(
                    &mut results,
                    &combined,
                    &file,
                    item,
                    "struct",
                    "Struct appears unused across project",
                );
            }
            for item in &enums {
                push_cross(
                    &mut results,
                    &combined,
                    &file,
                    item,
                    "enum",
                    "Enum appears unused across project",
                );
            }
            for item in &protocols {
                push_cross(
                    &mut results,
                    &combined,
                    &file,
                    item,
                    "protocol",
                    "Protocol appears unused across project",
                );
            }
            for item in &extensions {
                push_cross(
                    &mut results,
                    &combined,
                    &file,
                    item,
                    "extension",
                    "Extension appears unused across project",
                );
            }
            for item in &private_vars {
                if count_word(&content, &item.name) <= 1 {
                    push(
                        &mut results,
                        "var",
                        item,
                        &file,
                        "Private property is unused",
                    );
                }
            }
            for item in &private_funcs {
                if count_word(&content, &item.name) <= 1 {
                    push(
                        &mut results,
                        "function",
                        item,
                        &file,
                        "Private function is unused",
                    );
                }
            }
        }

        results
    }
}

fn detect_primary_language(project_path: &str) -> String {
    if !Path::new(project_path).exists() {
        return "C#".to_string();
    }

    let mut csharp = 0;
    let mut javascript = 0;
    let mut dart = 0;
    let mut swift = 0;

    for entry in WalkDir::new(project_path)
        .into_iter()
        .filter_entry(|entry| should_visit(entry, &[]))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        match entry
            .path()
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "cs" => csharp += 1,
            "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" => javascript += 1,
            "dart" => dart += 1,
            "swift" => swift += 1,
            _ => {}
        }
    }

    [
        ("C#", csharp),
        ("JavaScript", javascript),
        ("Dart", dart),
        ("Swift", swift),
    ]
    .into_iter()
    .max_by_key(|(_, count)| *count)
    .map(|(name, _)| name.to_string())
    .unwrap_or_else(|| "C#".to_string())
}

fn collect_files(
    project_path: &str,
    extensions: &[&str],
    extra_skip_dirs: &[&str],
    excluded_suffixes: &[&str],
) -> Vec<PathBuf> {
    if !Path::new(project_path).exists() {
        return Vec::new();
    }

    WalkDir::new(project_path)
        .into_iter()
        .filter_entry(|entry| should_visit(entry, extra_skip_dirs))
        .filter_map(Result::ok)
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
        .filter(|path| {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            !excluded_suffixes.iter().any(|suffix| {
                file_name
                    .to_ascii_lowercase()
                    .ends_with(&suffix.to_ascii_lowercase())
            })
        })
        .collect()
}

fn should_visit(entry: &DirEntry, extra_skip_dirs: &[&str]) -> bool {
    if entry.depth() == 0 || !entry.file_type().is_dir() {
        return true;
    }

    let name = entry.file_name().to_string_lossy();
    let lowered = name.to_ascii_lowercase();

    !name.starts_with('.')
        && !GLOBAL_SKIP_DIRS.contains(&lowered.as_str())
        && !extra_skip_dirs
            .iter()
            .any(|skip| skip.eq_ignore_ascii_case(lowered.as_str()))
}

fn read_combined(files: &[PathBuf]) -> String {
    files
        .iter()
        .filter_map(|file| fs::read_to_string(file).ok())
        .collect::<Vec<_>>()
        .join("\n")
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid analyzer regex")
}

fn capture(regex: &Regex, line: &str) -> Option<String> {
    regex
        .captures(line)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
}

fn count_word(text: &str, name: &str) -> usize {
    count_pattern(text, &format!(r"\b{}\b", regex::escape(name)))
}

fn count_pattern(text: &str, pattern: &str) -> usize {
    Regex::new(pattern)
        .ok()
        .map(|regex| regex.find_iter(text).count())
        .unwrap_or(0)
}

fn push_cross(
    results: &mut Vec<UnusedCodeResult>,
    combined: &str,
    file: &Path,
    symbol: &Symbol,
    kind: &str,
    hint: &str,
) {
    if count_word(combined, &symbol.name) <= 1 {
        push(results, kind, symbol, file, hint);
    }
}

fn push(results: &mut Vec<UnusedCodeResult>, kind: &str, symbol: &Symbol, file: &Path, hint: &str) {
    results.push(UnusedCodeResult {
        kind: kind.to_string(),
        name: symbol.name.clone(),
        location: format!("{}:{}", short_file_name(file), symbol.line),
        hints: vec![hint.to_string()],
    });
}

fn short_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

fn contains_ignore_case(values: &[&str], needle: &str) -> bool {
    values
        .iter()
        .any(|value| value.eq_ignore_ascii_case(needle))
}

fn starts_with_any(value: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| value.starts_with(prefix))
}

fn analyze_pubspec(project_path: &str) -> Vec<UnusedCodeResult> {
    let pubspec_path = Path::new(project_path).join("pubspec.yaml");
    let Ok(content) = fs::read_to_string(&pubspec_path) else {
        return Vec::new();
    };

    let combined_dart = read_combined(&collect_files(project_path, &["dart"], &[], &[]));
    let mut results = Vec::new();
    let skip_packages = ["flutter", "flutter_test", "meta", "collection", "async"];

    for package in parse_pubspec_dependencies(&content) {
        if skip_packages
            .iter()
            .any(|skip| skip.eq_ignore_ascii_case(&package))
        {
            continue;
        }
        if !combined_dart.contains(&format!("package:{package}")) {
            results.push(UnusedCodeResult {
                kind: "package".to_string(),
                name: package,
                location: "pubspec.yaml".to_string(),
                hints: vec!["Package defined but not imported in any Dart file".to_string()],
            });
        }
    }

    for asset in parse_pubspec_assets(&content) {
        if !asset_is_referenced(project_path, &asset, &combined_dart) {
            results.push(UnusedCodeResult {
                kind: "asset".to_string(),
                name: asset,
                location: "pubspec.yaml".to_string(),
                hints: vec!["Asset defined but not referenced in code".to_string()],
            });
        }
    }

    results
}

fn parse_pubspec_dependencies(content: &str) -> Vec<String> {
    let mut packages = Vec::new();
    let mut in_dependencies = false;

    for line in content.lines() {
        let trimmed = line.trim_start();

        if trimmed.starts_with("dependencies:") || trimmed.starts_with("dev_dependencies:") {
            in_dependencies = true;
            continue;
        }

        if in_dependencies
            && !line.starts_with(' ')
            && !line.starts_with('\t')
            && !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && trimmed.ends_with(':')
            && !trimmed.starts_with("dependencies")
            && !trimmed.starts_with("dev_dependencies")
        {
            in_dependencies = false;
        }

        if in_dependencies
            && line.starts_with("  ")
            && !line.starts_with("    ")
            && !trimmed.is_empty()
            && !trimmed.starts_with('#')
        {
            if let Some(index) = trimmed.find(':') {
                let name = trimmed[..index].trim();
                if !name.is_empty() && !name.contains(' ') && !name.eq_ignore_ascii_case("sdk") {
                    packages.push(name.to_string());
                }
            }
        }
    }

    packages
}

fn parse_pubspec_assets(content: &str) -> Vec<String> {
    let mut assets = Vec::new();
    let mut in_assets = false;

    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("assets:") {
            in_assets = true;
            continue;
        }
        if !in_assets {
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("- ") {
            let asset = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .split('#')
                .next()
                .unwrap_or_default()
                .trim();
            if !asset.is_empty() {
                assets.push(asset.to_string());
            }
            continue;
        }
        in_assets = false;
    }

    assets
}

fn asset_is_referenced(project_path: &str, asset: &str, combined_dart: &str) -> bool {
    let full_path = Path::new(project_path).join(asset.replace('/', "\\"));
    if full_path.is_file() {
        let file_name = short_file_name(&full_path);
        return combined_dart.contains(asset)
            || (!file_name.is_empty() && combined_dart.contains(&file_name))
            || Path::new(asset)
                .file_stem()
                .and_then(|name| name.to_str())
                .map(|name| combined_dart.contains(name))
                .unwrap_or(false);
    }

    if full_path.is_dir() {
        for entry in WalkDir::new(&full_path).into_iter().filter_map(Result::ok) {
            if entry.file_type().is_file() {
                let file_name = short_file_name(entry.path());
                if combined_dart.contains(asset) || combined_dart.contains(&file_name) {
                    return true;
                }
            }
        }
    }

    false
}

fn dedupe_and_sort(results: Vec<UnusedCodeResult>) -> Vec<UnusedCodeResult> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();

    for result in results {
        let key = format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}",
            result.kind,
            result.name,
            result.location,
            result.hints.join("|")
        );
        if seen.insert(key) {
            unique.push(result);
        }
    }

    unique.sort_by(|left, right| {
        left.location
            .cmp(&right.location)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.name.cmp(&right.name))
    });
    unique
}

#[cfg(test)]
mod tests {
    use super::UnusedCodeAnalyzer;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn csharp_unused_reports_private_symbols() {
        let dir = temp_project("csharp");
        fs::write(
            dir.join("Program.cs"),
            "class Program\n{\n    private int field;\n\n    private void Helper()\n    {\n    }\n}\n",
        )
        .expect("write Program.cs");

        let findings = UnusedCodeAnalyzer.analyze(dir.to_str().expect("utf8 path"));

        assert!(findings
            .iter()
            .any(|f| f.kind == "field" && f.name == "field" && f.location == "Program.cs:3"));
        assert!(findings
            .iter()
            .any(|f| f.kind == "method" && f.name == "Helper" && f.location == "Program.cs:5"));

        fs::remove_dir_all(dir).expect("cleanup temp dir");
    }

    #[test]
    fn dart_pubspec_reports_unused_items() {
        let dir = temp_project("dart");
        fs::create_dir_all(dir.join("assets")).expect("create assets dir");
        fs::write(
            dir.join("pubspec.yaml"),
            "dependencies:\n  http: ^1.0.0\n\nflutter:\n  assets:\n    - assets/logo.png\n",
        )
        .expect("write pubspec.yaml");
        fs::write(dir.join("assets").join("logo.png"), "fake").expect("write asset");
        fs::write(dir.join("main.dart"), "void main() { print('hello'); }\n")
            .expect("write main.dart");

        let findings = UnusedCodeAnalyzer.analyze(dir.to_str().expect("utf8 path"));

        assert!(findings
            .iter()
            .any(|f| f.kind == "package" && f.name == "http"));
        assert!(findings
            .iter()
            .any(|f| f.kind == "asset" && f.name == "assets/logo.png"));

        fs::remove_dir_all(dir).expect("cleanup temp dir");
    }

    fn temp_project(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("devatlas-cli-{prefix}-{nanos}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }
}
