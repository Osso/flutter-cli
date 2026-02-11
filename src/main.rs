mod commands;
mod config;
mod isolate;
mod process;
mod snapshot;
mod state;
mod vm_service;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "flutter-cli")]
#[command(about = "Flutter app inspection CLI using Dart VM Service Protocol")]
struct Cli {
    /// VM Service WebSocket URL (bypasses process management)
    #[arg(long)]
    url: Option<String>,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Flutter project directory (defaults to cwd)
    #[arg(long)]
    project_dir: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Widget tree as indented text
    Snapshot {
        /// Maximum tree depth
        #[arg(short, long)]
        depth: Option<usize>,
        /// Filter by widget name (substring or glob with *)
        #[arg(short, long)]
        filter: Option<String>,
        /// Skip framework-internal widgets
        #[arg(short, long)]
        compact: bool,
    },
    /// Take a screenshot (PNG)
    Screenshot {
        /// Widget valueId to screenshot (whole app if omitted)
        #[arg(long)]
        id: Option<String>,
        /// Output path
        #[arg(default_value = "/tmp/claude/flutter-screenshot.png")]
        path: String,
    },
    /// Widget properties
    Details {
        /// Widget valueId from snapshot output
        value_id: String,
        /// Subtree depth
        #[arg(short, long, default_value_t = 2)]
        depth: usize,
    },
    /// Layout constraints, sizes, flex
    Layout {
        /// Widget valueId from snapshot output
        value_id: String,
    },
    /// Render tree (text dump)
    DumpRender,
    /// Semantics tree (text dump)
    DumpSemantics,
    /// Hot reload
    Reload,
    /// Hot restart
    Restart,
    /// Connection info
    Status,
    /// Kill managed flutter run process
    Stop,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let project_dir = cli.project_dir.clone();
    let json = cli.json;

    match cli.command {
        Command::Snapshot { depth, filter, compact } => {
            commands::cmd_snapshot(project_dir, cli.url, depth, filter, compact, json).await
        }
        Command::Screenshot { id, path } => {
            commands::cmd_screenshot(project_dir, cli.url, id, &path, json).await
        }
        Command::Details { value_id, depth } => {
            commands::cmd_details(project_dir, cli.url, &value_id, depth, json).await
        }
        Command::Layout { value_id } => {
            commands::cmd_layout(project_dir, cli.url, &value_id, json).await
        }
        Command::DumpRender => {
            commands::cmd_dump_render(project_dir, cli.url, json).await
        }
        Command::DumpSemantics => {
            commands::cmd_dump_semantics(project_dir, cli.url, json).await
        }
        Command::Reload => commands::cmd_reload(project_dir, cli.url, json).await,
        Command::Restart => commands::cmd_restart(project_dir, cli.url, json).await,
        Command::Status => commands::cmd_status(project_dir, cli.url, json).await,
        Command::Stop => commands::cmd_stop(project_dir).await,
    }
}
