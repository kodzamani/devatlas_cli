use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::error;

mod analyzer;
mod cache;
mod commands;
mod dependencies;
mod editor;
mod git_stats;
mod models;
mod runner;
mod scanner;
mod settings;
mod unused;

use models::DateRangeFilter;

#[derive(Parser)]
#[command(name = "devatlas")]
#[command(about = "DevAtlas desktop app parity CLI")]
#[command(version = "0.2.0")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(short, long, global = true)]
    verbose: bool,

    #[arg(short, long, global = true)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Command {
    #[command(alias = "ls")]
    List {
        #[arg(short, long)]
        category: Option<String>,
        #[arg(short, long)]
        search: Option<String>,
        #[arg(long)]
        active_only: bool,
        #[arg(long)]
        rescan: bool,
    },
    Scan {
        #[arg(short, long)]
        drive: Option<String>,
        #[arg(short, long)]
        path: Option<String>,
    },
    Open {
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        path: Option<String>,
        #[arg(short, long)]
        editor: Option<String>,
        #[arg(long)]
        select: bool,
        #[arg(long)]
        rescan: bool,
    },
    Run {
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        path: Option<String>,
        #[arg(short, long)]
        script: Option<String>,
        #[arg(long)]
        detached: bool,
        #[arg(long)]
        install: bool,
        #[arg(long)]
        open_browser: bool,
        #[arg(long)]
        rescan: bool,
    },
    Analyze {
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        path: Option<String>,
        #[arg(long)]
        files: bool,
        #[arg(long)]
        tech_stack: bool,
        #[arg(long)]
        rescan: bool,
    },
    Dependencies {
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        path: Option<String>,
        #[arg(long)]
        check_updates: bool,
        #[arg(long)]
        rescan: bool,
    },
    Stats {
        #[arg(short, long, value_enum, default_value_t = DateRangeFilter::Month)]
        range: DateRangeFilter,
        #[arg(short, long)]
        project: Option<String>,
        #[arg(long, default_value_t = 10)]
        top: usize,
        #[arg(long)]
        rescan: bool,
    },
    UnusedCode {
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        path: Option<String>,
        #[arg(long)]
        rescan: bool,
    },
    Status,
    ClearCache,
    Onboarding {
        #[command(subcommand)]
        command: OnboardingCommand,
    },
}

#[derive(Subcommand)]
enum OnboardingCommand {
    Status,
    Complete,
    Reset,
    Tour,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose, cli.quiet);

    let Some(command) = cli.command else {
        print_welcome();
        return Ok(());
    };

    let result = match command {
        Command::List {
            category,
            search,
            active_only,
            rescan,
        } => commands::list_projects(category, search, active_only, rescan).await,
        Command::Scan { drive, path } => commands::scan(path, drive).await,
        Command::Open {
            name,
            path,
            editor,
            select,
            rescan,
        } => commands::open_project(name, path, editor, select, rescan).await,
        Command::Run {
            name,
            path,
            script,
            detached,
            install,
            open_browser,
            rescan,
        } => {
            commands::run_project(name, path, script, detached, install, open_browser, rescan).await
        }
        Command::Analyze {
            name,
            path,
            files,
            tech_stack,
            rescan,
        } => commands::analyze_project(name, path, files, tech_stack, rescan).await,
        Command::Dependencies {
            name,
            path,
            check_updates,
            rescan,
        } => commands::dependencies(name, path, check_updates, rescan).await,
        Command::Stats {
            range,
            project,
            top,
            rescan,
        } => commands::stats(range, project, top, rescan).await,
        Command::UnusedCode { name, path, rescan } => {
            commands::unused_code(name, path, rescan).await
        }
        Command::Status => commands::status().await,
        Command::ClearCache => commands::clear_cache().await,
        Command::Onboarding { command } => match command {
            OnboardingCommand::Status => commands::onboarding_status(),
            OnboardingCommand::Complete => commands::onboarding_complete(),
            OnboardingCommand::Reset => commands::onboarding_reset(),
            OnboardingCommand::Tour => {
                commands::onboarding_tour();
                Ok(())
            }
        },
    };

    if let Err(err) = result {
        error!("{err}");
        eprintln!("Error: {err}");
        std::process::exit(1);
    }

    Ok(())
}

fn init_tracing(verbose: bool, quiet: bool) {
    let level = if quiet {
        "error"
    } else if verbose {
        "debug"
    } else {
        "info"
    };

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive(
                level
                    .parse()
                    .unwrap_or_else(|_| tracing::Level::INFO.into()),
            ),
        )
        .try_init();
}

fn print_welcome() {
    println!("DevAtlas CLI");
    println!();
    println!("Core commands:");
    println!("  devatlas scan");
    println!("  devatlas list");
    println!("  devatlas open --name <project>");
    println!("  devatlas analyze --name <project>");
    println!("  devatlas dependencies --name <project> --check-updates");
    println!("  devatlas stats --range month");
    println!("  devatlas status");
}
