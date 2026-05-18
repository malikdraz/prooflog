use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};
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
        Command::Init(args) => init_config(args)?,
        Command::Doctor(args) => doctor_config(args)?,
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

fn init_config(args: InitArgs) -> Result<()> {
    let paths = ProoflogPaths::resolve()?;
    let config = if paths.config_file.exists() {
        ensure_config_file_path(&paths.config_file)?;
        ProoflogConfig::read(&paths.config_file)?
    } else {
        ProoflogConfig::defaults(&paths)?
    }
    .with_overrides(args.db, args.codex_root);

    config.write_if_missing(&paths.config_file)?;
    print_config_status("Config:", &paths.config_file, &config);
    println!("Status:");
    println!("  config created or already present");
    Ok(())
}

fn doctor_config(args: DoctorArgs) -> Result<()> {
    let paths = ProoflogPaths::resolve()?;
    let config = ProoflogConfig::read(&paths.config_file)
        .with_context(|| {
            format!(
                "run `prooflog init` to create {}",
                paths.config_file.display()
            )
        })?
        .with_overrides(args.db, args.codex_root);

    print_config_status("Config:", &paths.config_file, &config);
    println!("Status:");
    println!("  config ok");
    Ok(())
}

fn print_config_status(heading: &str, config_file: &Path, config: &ProoflogConfig) {
    println!("{heading}");
    println!("  path: {}", config_file.display());
    println!("  db: {}", config.db_path.display());
    println!("  codex root: {}", config.codex_root.display());
    println!(
        "  redaction: secrets={}, local_paths={}",
        config.redaction.redact_secrets, config.redaction.redact_local_paths
    );
}

fn ensure_config_file_path(path: &Path) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(ConfigError::InvalidConfigPath {
            path: path.to_path_buf(),
        })
        .context("invalid config path")
    }
}

#[derive(Debug)]
struct ProoflogPaths {
    config_file: PathBuf,
    db_file: PathBuf,
    codex_root: PathBuf,
}

impl ProoflogPaths {
    fn resolve() -> Result<Self> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or(ConfigError::MissingHome)
            .context("failed to resolve ProofLog local paths")?;

        let config_home = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        let data_home = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("share"));

        Ok(Self {
            config_file: config_home.join("prooflog").join("config.toml"),
            db_file: data_home.join("prooflog").join("prooflog.db"),
            codex_root: home.join(".codex"),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProoflogConfig {
    db_path: PathBuf,
    codex_root: PathBuf,
    redaction: RedactionConfig,
}

impl ProoflogConfig {
    fn defaults(paths: &ProoflogPaths) -> Result<Self> {
        Ok(Self {
            db_path: paths.db_file.clone(),
            codex_root: paths.codex_root.clone(),
            redaction: RedactionConfig {
                redact_secrets: true,
                redact_local_paths: true,
            },
        })
    }

    fn read(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .map_err(|source| ConfigError::ReadConfig {
                path: path.to_path_buf(),
                source,
            })
            .context("failed to read config")?;
        toml::from_str(&text)
            .map_err(|source| ConfigError::ParseConfig {
                path: path.to_path_buf(),
                source,
            })
            .context("failed to parse config")
    }

    fn write_if_missing(&self, path: &Path) -> Result<()> {
        if path.exists() && path.is_file() {
            return Ok(());
        }

        let parent = path
            .parent()
            .ok_or_else(|| ConfigError::InvalidConfigPath {
                path: path.to_path_buf(),
            })?;
        fs::create_dir_all(parent)
            .map_err(|source| ConfigError::CreateConfigDir {
                path: parent.to_path_buf(),
                source,
            })
            .context("failed to create config directory")?;

        let text = toml::to_string_pretty(self).context("failed to serialize config")?;
        fs::write(path, text)
            .map_err(|source| ConfigError::WriteConfig {
                path: path.to_path_buf(),
                source,
            })
            .context("failed to write config")
    }

    fn with_overrides(mut self, db_path: Option<PathBuf>, codex_root: Option<PathBuf>) -> Self {
        if let Some(db_path) = db_path {
            self.db_path = db_path;
        }
        if let Some(codex_root) = codex_root {
            self.codex_root = codex_root;
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RedactionConfig {
    redact_secrets: bool,
    redact_local_paths: bool,
}

#[derive(Debug, thiserror::Error)]
enum ConfigError {
    #[error(
        "HOME is not set; set HOME or XDG_CONFIG_HOME and XDG_DATA_HOME before running ProofLog"
    )]
    MissingHome,
    #[error("invalid config path: {path}")]
    InvalidConfigPath { path: PathBuf },
    #[error("could not create config directory {path}")]
    CreateConfigDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not read config file {path}")]
    ReadConfig {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not parse config file {path}")]
    ParseConfig {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("could not write config file {path}")]
    WriteConfig {
        path: PathBuf,
        source: std::io::Error,
    },
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

    /// Override the Codex history root.
    #[arg(long, value_name = "PATH")]
    codex_root: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct DoctorArgs {
    /// Override the local ProofLog database path.
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,

    /// Override the Codex history root.
    #[arg(long, value_name = "PATH")]
    codex_root: Option<PathBuf>,
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

    /// Override the Codex history root.
    #[arg(long, value_name = "PATH")]
    codex_root: Option<PathBuf>,
}
