# DevAtlas CLI

DevAtlas CLI is a lightweight command-line tool that scans and manages your development projects from one place.

It is built for developers who work across many repositories, experiments, and mixed technology stacks. It discovers projects, classifies them, caches results, opens them quickly, and gives you a practical overview of your local workspace.

## What It Offers

- Automatically scans projects on your machine
- Lists projects by type and category
- Opens projects in a detected editor with a single command
- Finds runnable commands for supported web projects
- Analyzes files, line counts, and language distribution
- Reads dependency manifests and checks for updates
- Generates git activity and project statistics
- Flags likely unused code
- Stores scan results in cache

## Highlighted Features

### Smart project discovery
Recognizes markers such as `Cargo.toml`, `package.json`, `requirements.txt`, `go.mod`, `.sln`, `pom.xml`, and `Dockerfile` to detect Rust, Node.js, Python, .NET, Java, Go, Flutter, and similar project types.

### Fast access through caching
Instead of scanning the disk from scratch every time, DevAtlas reuses cached data when possible. That makes listing, opening, and analysis workflows much faster.

### Built for daily developer workflows
You can find a project by name, open it instantly, run it, inspect dependencies, or generate a quick technical summary without digging through folders manually.

## Quick Start

### Requirements

- Rust toolchain
- Windows is the recommended environment
- Optional: VS Code, Cursor, Windsurf, or Antigravity

### Build

```bash
cargo build --release
```

Run the first scan:

```bash
cargo run -- scan
```

Or use the compiled binary directly:

```bash
target/release/devatlas_cli.exe
```

## Basic Usage

Start the first scan:

```bash
devatlas_cli.exe scan
```

Scan a specific directory:

```bash
devatlas_cli.exe scan --path D:\Projects
```

List projects:

```bash
devatlas_cli.exe list
```

Search projects:

```bash
devatlas_cli.exe list --search api
```

Open a project:

```bash
devatlas_cli.exe open --name devatlas_cli
```

Analyze a project:

```bash
devatlas_cli.exe analyze --name devatlas_cli --tech-stack
```

Check dependency updates:

```bash
devatlas_cli.exe dependencies --name devatlas_cli --check-updates
```

Get a stats summary:

```bash
devatlas_cli.exe stats --range month
```

## Commands

| Command | Description |
| --- | --- |
| `scan` | Scans your disks or a specific path for projects |
| `list` | Lists discovered projects with filtering options |
| `open` | Opens a project in a supported editor |
| `run` | Starts a detected run command for a project |
| `analyze` | Shows file, line, and technology summaries |
| `dependencies` | Inspects dependency manifests |
| `stats` | Displays overall project and git statistics |
| `unused-code` | Reports likely unused code |
| `status` | Shows cache and editor status |
| `clear-cache` | Clears the project cache |

## Who Is It For?

- Developers managing many local repositories
- Teams working across multiple languages and frameworks
- Anyone trying to reduce the “where was that project?” problem
- Developers who want a quick overview of active work, older repos, and technology spread

## Notes

- The tool uses caching; run `scan` again when you want a fresh index.
- Editor integration depends on which supported editors are installed.
- The best experience is currently targeted at Windows.
