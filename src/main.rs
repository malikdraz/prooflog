use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use rusqlite::{Connection, OpenFlags};
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
    let storage = initialize_database(&config.db_path)?;
    print_config_status("Config:", &paths.config_file, &config);
    print_storage_status(&storage);
    println!("Status:");
    println!("  init ok");
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
    let storage = inspect_database(&config.db_path)?;

    print_config_status("Config:", &paths.config_file, &config);
    print_storage_status(&storage);
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

fn print_storage_status(status: &StorageStatus) {
    println!("Storage:");
    println!("  db: {}", status.db_path.display());
    println!("  sqlite: ok");
    println!("  migration: {}", status.migration_version);
    println!("  fts5: ok");
    println!("  journal: {}", status.journal_mode);
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
struct StorageStatus {
    db_path: PathBuf,
    migration_version: i64,
    journal_mode: String,
}

fn initialize_database(db_path: &Path) -> Result<StorageStatus> {
    let parent = db_path
        .parent()
        .ok_or_else(|| StorageError::InvalidDbPath {
            path: db_path.to_path_buf(),
        })?;
    fs::create_dir_all(parent)
        .map_err(|source| StorageError::CreateDbDir {
            path: parent.to_path_buf(),
            source,
        })
        .with_context(|| format!("failed to initialize database {}", db_path.display()))?;

    let conn = open_database(db_path)?;
    apply_schema(&conn, db_path)?;
    storage_status(&conn, db_path)
}

fn inspect_database(db_path: &Path) -> Result<StorageStatus> {
    let conn = open_existing_database(db_path)?;
    storage_status(&conn, db_path)
}

fn open_database(db_path: &Path) -> Result<Connection> {
    if db_path.is_dir() {
        return Err(StorageError::InvalidDbPath {
            path: db_path.to_path_buf(),
        })
        .with_context(|| format!("failed to initialize database {}", db_path.display()));
    }

    let conn = Connection::open(db_path)
        .map_err(|source| StorageError::OpenDb {
            path: db_path.to_path_buf(),
            source,
        })
        .with_context(|| format!("failed to initialize database {}", db_path.display()))?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|source| StorageError::ConfigureDb {
            path: db_path.to_path_buf(),
            source,
        })
        .with_context(|| format!("failed to initialize database {}", db_path.display()))?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|source| StorageError::ConfigureDb {
            path: db_path.to_path_buf(),
            source,
        })
        .with_context(|| format!("failed to initialize database {}", db_path.display()))?;
    Ok(conn)
}

fn open_existing_database(db_path: &Path) -> Result<Connection> {
    if db_path.is_dir() {
        return Err(StorageError::InvalidDbPath {
            path: db_path.to_path_buf(),
        })
        .with_context(|| format!("failed to inspect database {}", db_path.display()));
    }

    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|source| StorageError::OpenDb {
        path: db_path.to_path_buf(),
        source,
    })
    .with_context(|| format!("failed to inspect database {}", db_path.display()))?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|source| StorageError::ConfigureDb {
            path: db_path.to_path_buf(),
            source,
        })
        .with_context(|| format!("failed to inspect database {}", db_path.display()))?;
    Ok(conn)
}

fn apply_schema(conn: &Connection, db_path: &Path) -> Result<()> {
    conn.execute_batch(SCHEMA_V1)
        .map_err(|source| StorageError::ApplySchema {
            path: db_path.to_path_buf(),
            source,
        })
        .with_context(|| format!("failed to initialize database {}", db_path.display()))?;
    Ok(())
}

fn storage_status(conn: &Connection, db_path: &Path) -> Result<StorageStatus> {
    let migration_version = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|source| StorageError::InspectDb {
            path: db_path.to_path_buf(),
            source,
        })
        .with_context(|| format!("failed to inspect database {}", db_path.display()))?;
    let journal_mode = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
        .map_err(|source| StorageError::InspectDb {
            path: db_path.to_path_buf(),
            source,
        })
        .with_context(|| format!("failed to inspect database {}", db_path.display()))?;
    conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS prooflog_fts5_probe USING fts5(value)",
        [],
    )
    .map_err(|source| StorageError::FtsUnavailable {
        path: db_path.to_path_buf(),
        source,
    })
    .with_context(|| format!("failed to inspect database {}", db_path.display()))?;
    conn.execute("DROP TABLE IF EXISTS prooflog_fts5_probe", [])
        .map_err(|source| StorageError::InspectDb {
            path: db_path.to_path_buf(),
            source,
        })
        .with_context(|| format!("failed to inspect database {}", db_path.display()))?;

    Ok(StorageStatus {
        db_path: db_path.to_path_buf(),
        migration_version,
        journal_mode,
    })
}

const SCHEMA_V1: &str = r#"
PRAGMA user_version = 1;

CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT OR IGNORE INTO schema_migrations (version) VALUES (1);

CREATE TABLE IF NOT EXISTS codex_files (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    size_bytes INTEGER,
    modified_at TEXT,
    sha256 TEXT,
    ingested_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY,
    codex_session_id TEXT UNIQUE,
    workspace_path TEXT,
    model TEXT,
    title TEXT,
    started_at TEXT,
    ended_at TEXT,
    event_count INTEGER NOT NULL DEFAULT 0,
    parse_status TEXT NOT NULL DEFAULT 'unknown'
);

CREATE TABLE IF NOT EXISTS raw_events (
    id INTEGER PRIMARY KEY,
    codex_file_id INTEGER NOT NULL REFERENCES codex_files(id) ON DELETE CASCADE,
    line_number INTEGER NOT NULL,
    raw_json TEXT NOT NULL,
    line_sha256 TEXT NOT NULL UNIQUE,
    event_type TEXT,
    event_time TEXT,
    session_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL,
    parse_error TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (codex_file_id, line_number)
);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY,
    raw_event_id INTEGER NOT NULL REFERENCES raw_events(id) ON DELETE CASCADE,
    session_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL,
    role TEXT,
    text TEXT,
    created_at TEXT
);

CREATE TABLE IF NOT EXISTS commands (
    id INTEGER PRIMARY KEY,
    raw_event_id INTEGER NOT NULL REFERENCES raw_events(id) ON DELETE CASCADE,
    session_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL,
    command TEXT NOT NULL,
    cwd TEXT,
    status TEXT,
    exit_code INTEGER,
    output TEXT,
    started_at TEXT,
    ended_at TEXT
);

CREATE TABLE IF NOT EXISTS approvals (
    id INTEGER PRIMARY KEY,
    raw_event_id INTEGER NOT NULL REFERENCES raw_events(id) ON DELETE CASCADE,
    session_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL,
    action TEXT,
    decision TEXT,
    sandbox_mode TEXT,
    command TEXT,
    created_at TEXT
);

CREATE TABLE IF NOT EXISTS file_changes (
    id INTEGER PRIMARY KEY,
    raw_event_id INTEGER NOT NULL REFERENCES raw_events(id) ON DELETE CASCADE,
    session_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL,
    path TEXT NOT NULL,
    change_type TEXT,
    diff_text TEXT,
    lines_added INTEGER,
    lines_deleted INTEGER
);

CREATE TABLE IF NOT EXISTS proof_facts (
    id INTEGER PRIMARY KEY,
    session_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL,
    command_id INTEGER REFERENCES commands(id) ON DELETE SET NULL,
    file_change_id INTEGER REFERENCES file_changes(id) ON DELETE SET NULL,
    kind TEXT NOT NULL,
    subject TEXT,
    status TEXT NOT NULL,
    reason TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE VIRTUAL TABLE IF NOT EXISTS raw_events_fts
USING fts5(raw_json, parse_error, content='raw_events', content_rowid='id');

CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts
USING fts5(text, content='messages', content_rowid='id');

CREATE VIRTUAL TABLE IF NOT EXISTS command_output_fts
USING fts5(command, output, content='commands', content_rowid='id');
"#;

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

#[derive(Debug, thiserror::Error)]
enum StorageError {
    #[error("invalid database path: {path}")]
    InvalidDbPath { path: PathBuf },
    #[error("could not create database directory {path}")]
    CreateDbDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not open database {path}")]
    OpenDb {
        path: PathBuf,
        source: rusqlite::Error,
    },
    #[error("could not configure database {path}")]
    ConfigureDb {
        path: PathBuf,
        source: rusqlite::Error,
    },
    #[error("could not apply database schema {path}")]
    ApplySchema {
        path: PathBuf,
        source: rusqlite::Error,
    },
    #[error("could not inspect database {path}")]
    InspectDb {
        path: PathBuf,
        source: rusqlite::Error,
    },
    #[error("SQLite FTS5 is unavailable for database {path}")]
    FtsUnavailable {
        path: PathBuf,
        source: rusqlite::Error,
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
