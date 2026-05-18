use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use tracing::debug;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();
    debug!(command = ?cli.command, "parsed cli arguments");
    run(cli)
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .try_init();
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Init(args) => print_placeholder("init", args.db.as_ref()),
        Command::Doctor(args) => print_placeholder("doctor", args.db.as_ref()),
        Command::Ingest(args) => {
            println!(
                "prooflog ingest is not implemented yet. Planned Codex root: {}",
                args.codex_root
                    .as_ref()
                    .map_or("<default>".to_string(), |path| path.display().to_string())
            );
        }
        Command::Proof(args) => {
            println!(
                "prooflog proof is not implemented yet. Planned comparison base: {}",
                args.since
            );
        }
    }

    Ok(())
}

fn print_placeholder(command: &str, db: Option<&PathBuf>) {
    let db = db
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "<default>".to_string());
    println!("prooflog {command} is not implemented yet. Planned DB path: {db}");
}

#[derive(Debug, Parser)]
#[command(
    name = "prooflog",
    version,
    about = "Local-first proof reports for agent-assisted code changes.",
    long_about = "Local-first proof reports for agent-assisted code changes.\n\nProofLog reads local Codex history and git state to produce deterministic proof reports for senior engineers."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Prepare local ProofLog config and storage.
    Init(InitArgs),
    /// Check local ProofLog readiness.
    Doctor(DoctorArgs),
    /// Ingest local session history.
    Ingest(IngestArgs),
    /// Produce a proof report for changes since a git ref.
    Proof(ProofArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    /// Override the local ProofLog database path.
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct DoctorArgs {
    /// Override the local ProofLog database path.
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct IngestArgs {
    /// Ingest local Codex JSONL history.
    #[arg(long, required = true)]
    codex: bool,

    /// Override the Codex history root.
    #[arg(long, value_name = "PATH")]
    codex_root: Option<PathBuf>,

    /// Override the local ProofLog database path.
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ProofArgs {
    /// Git ref used as the comparison base.
    #[arg(long, value_name = "REF")]
    since: String,

    /// Repository path to inspect.
    #[arg(long, value_name = "PATH")]
    repo: Option<PathBuf>,

    /// Override the local ProofLog database path.
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,
}
