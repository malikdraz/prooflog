#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
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
        Command::Ingest(args) => ingest_codex(args)?,
        Command::Proof(args) => proof_git_context(args)?,
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
    enforce_owner_only_permissions(&paths.config_file)?;
    enforce_owner_only_permissions(&config.db_path)?;
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
    let codex = inspect_codex(&config.codex_root);
    let git = inspect_git();
    let mut warnings = permission_warnings(&paths.config_file, &config.db_path)?;
    warnings.extend(codex.warnings.clone());
    warnings.extend(git.warnings.clone());

    print_config_status("Config:", &paths.config_file, &config);
    print_storage_status(&storage);
    print_codex_status(&codex);
    print_git_status(&git);
    print_warnings(&warnings);
    println!("Status:");
    println!("  config ok");
    Ok(())
}

fn ingest_codex(args: IngestArgs) -> Result<()> {
    let paths = ProoflogPaths::resolve()?;
    let config = ProoflogConfig::read(&paths.config_file)
        .with_context(|| {
            format!(
                "run `prooflog init` to create {}",
                paths.config_file.display()
            )
        })?
        .with_overrides(args.db, args.codex_root);
    let mut conn = open_existing_database(&config.db_path)
        .with_context(|| "run `prooflog init` before ingesting Codex history")?;
    apply_schema(&conn, &config.db_path)?;
    let summary = discover_and_record_codex_files(&mut conn, &config.codex_root)?;

    println!("Codex ingest:");
    println!("  root: {}", config.codex_root.display());
    println!("  files discovered: {}", summary.discovered);
    println!("  files ingested: {}", summary.recorded);
    println!("  files skipped: {}", summary.skipped);
    println!("  raw events stored: {}", summary.raw_events_stored);
    println!("  raw events skipped: {}", summary.raw_events_skipped);
    println!("  malformed lines: {}", summary.malformed_lines);
    println!("  unknown event shapes: {}", summary.unknown_event_shapes);
    println!("  warnings: {}", summary.warnings.len());
    if !summary.warnings.is_empty() {
        println!("Warnings:");
        for warning in summary.warnings {
            println!("  {warning}");
        }
    }

    Ok(())
}

fn proof_git_context(args: ProofArgs) -> Result<()> {
    let since = args.since;
    let db_path = resolve_proof_db_path(args.db)?;
    let repo_path = args.repo.unwrap_or(env::current_dir().context(
        "failed to resolve current directory; pass --repo <PATH> to choose a repository",
    )?);
    let git = inspect_proof_git(&repo_path, &since)?;
    let changed = inspect_changed_files(&git.repo_root, &git.merge_base)?;
    let correlation = correlate_sessions(&db_path, &git.repo_root, &changed)?;

    println!("Git:");
    println!("  repo: {}", git.repo_root.display());
    println!("  branch: {}", git.branch);
    println!("  head: {}", git.head);
    println!("  merge base: {}", git.merge_base);
    println!("  dirty: {}", if git.dirty { "yes" } else { "no" });
    print_changed_files(&changed);
    let risk = classify_changed_risks(&changed);
    print_risk_report(&risk);
    let risky_commands = classify_risky_commands(&db_path, &correlation)?;
    print_risky_commands(&risky_commands);
    print_session_correlation(&correlation);
    let decision = decide_proof(&db_path, &changed, &correlation)?;
    print_proof_decision(&decision);
    println!("Proof:");
    println!("  since: {since}");
    println!("  proof report: not implemented yet");
    Ok(())
}

fn resolve_proof_db_path(db_path: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(db_path) = db_path {
        return Ok(db_path);
    }

    let paths = ProoflogPaths::resolve()?;
    Ok(ProoflogConfig::read(&paths.config_file)
        .map(|config| config.db_path)
        .unwrap_or(paths.db_file))
}

fn print_changed_files(changed: &ChangedFiles) {
    println!("Changed:");
    println!("  files: {}", changed.files.len());
    println!("  additions: {}", changed.total_additions);
    println!("  deletions: {}", changed.total_deletions);
    println!(
        "  docs only: {}",
        if changed.docs_only { "yes" } else { "no" }
    );
    for file in &changed.files {
        println!(
            "  {} {} (+{} -{})",
            file.status,
            file.display_path(),
            display_stat(file.additions),
            display_stat(file.deletions)
        );
    }
}

fn print_risk_report(risk: &RiskReport) {
    println!("Risk:");
    println!("  risk level: {}", risk.level);
    println!("  risky files: {}", risk.risky_file_count);
    for finding in &risk.findings {
        println!(
            "  {}: {} ({})",
            finding.category, finding.path, finding.reason
        );
    }
}

fn print_risky_commands(report: &RiskyCommandReport) {
    println!("Risky commands:");
    println!("  relevant: {}", report.relevant.len());
    println!("  ambiguous: {}", report.ambiguous.len());
    for finding in &report.relevant {
        println!(
            "  {} {} {} {}: {}",
            finding.severity,
            finding.family,
            finding.codex_session_id,
            finding.session_title.as_deref().unwrap_or("(untitled)"),
            finding.command
        );
        println!("    reason: {}", finding.reason);
    }
    for finding in &report.ambiguous {
        println!(
            "  ambiguous {} {} {} {}: {}",
            finding.severity,
            finding.family,
            finding.codex_session_id,
            finding.session_title.as_deref().unwrap_or("(untitled)"),
            finding.command
        );
        println!("    reason: {}", finding.reason);
    }
}

fn print_session_correlation(correlation: &SessionCorrelation) {
    println!("Codex:");
    println!("  relevant sessions: {}", correlation.relevant.len());
    println!("  ambiguous sessions: {}", correlation.ambiguous.len());
    for session in &correlation.relevant {
        println!(
            "  {} {} [{}]",
            session.codex_session_id,
            session.title.as_deref().unwrap_or("(untitled)"),
            session.signals.join(", ")
        );
    }
    for session in &correlation.ambiguous {
        println!(
            "  {} {} [{}]",
            session.codex_session_id,
            session.title.as_deref().unwrap_or("(untitled)"),
            session.signals.join(", ")
        );
    }
}

fn print_proof_decision(decision: &ProofDecision) {
    println!("Decision:");
    println!("  status: {}", decision.status);
    for reason in &decision.reasons {
        println!("  reason: {reason}");
    }
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

fn print_codex_status(status: &CodexStatus) {
    println!("Codex:");
    println!("  root: {}", status.root_state);
    println!("  path: {}", status.root.display());
    println!("  jsonl files: {}", status.jsonl_files);
}

fn print_git_status(status: &GitStatus) {
    println!("Git:");
    match &status.repo_root {
        Some(repo_root) => println!("  repo: {}", repo_root.display()),
        None => println!("  repo: not detected"),
    }
    if let Some(branch) = &status.branch {
        println!("  branch: {branch}");
    }
}

fn print_warnings(warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }

    println!("Warnings:");
    for warning in warnings {
        println!("  {warning}");
    }
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

#[derive(Debug)]
struct CodexStatus {
    root: PathBuf,
    root_state: &'static str,
    jsonl_files: usize,
    warnings: Vec<String>,
}

#[derive(Debug)]
struct GitStatus {
    repo_root: Option<PathBuf>,
    branch: Option<String>,
    warnings: Vec<String>,
}

#[derive(Debug)]
struct ProofGitContext {
    repo_root: PathBuf,
    branch: String,
    head: String,
    merge_base: String,
    dirty: bool,
}

#[derive(Debug)]
struct ChangedFiles {
    files: Vec<ChangedFile>,
    total_additions: i64,
    total_deletions: i64,
    docs_only: bool,
}

#[derive(Debug)]
struct ChangedFile {
    status: String,
    path: String,
    previous_path: Option<String>,
    additions: Option<i64>,
    deletions: Option<i64>,
}

impl ChangedFile {
    fn display_path(&self) -> String {
        match &self.previous_path {
            Some(previous_path) => format!("{previous_path} -> {}", self.path),
            None => self.path.clone(),
        }
    }
}

#[derive(Debug)]
struct RiskReport {
    level: &'static str,
    risky_file_count: usize,
    findings: Vec<RiskFinding>,
}

#[derive(Debug)]
struct RiskFinding {
    category: &'static str,
    path: String,
    reason: &'static str,
}

#[derive(Debug, Default)]
struct RiskyCommandReport {
    relevant: Vec<RiskyCommandFinding>,
    ambiguous: Vec<RiskyCommandFinding>,
}

#[derive(Debug)]
struct RiskyCommandFinding {
    codex_session_id: String,
    session_title: Option<String>,
    family: &'static str,
    severity: &'static str,
    reason: &'static str,
    command: String,
}

#[derive(Debug)]
struct ProofDecision {
    status: &'static str,
    reasons: Vec<String>,
}

#[derive(Debug)]
struct DecisionFact {
    session_id: i64,
    codex_session_id: String,
    command_id: Option<i64>,
    kind: String,
    subject: Option<String>,
    status: String,
}

#[derive(Debug, Default)]
struct SessionCorrelation {
    relevant: Vec<CorrelatedSession>,
    ambiguous: Vec<CorrelatedSession>,
}

#[derive(Debug)]
struct CorrelatedSession {
    id: i64,
    codex_session_id: String,
    title: Option<String>,
    signals: Vec<String>,
}

#[derive(Debug)]
struct StoredSession {
    id: i64,
    codex_session_id: String,
    title: Option<String>,
    workspace_path: Option<String>,
    command_cwds: Vec<String>,
    file_paths: Vec<String>,
}

#[derive(Debug, Default)]
struct IngestSummary {
    discovered: usize,
    recorded: usize,
    skipped: usize,
    raw_events_stored: usize,
    raw_events_skipped: usize,
    malformed_lines: usize,
    unknown_event_shapes: usize,
    warnings: Vec<String>,
}

#[derive(Debug)]
struct DiscoveredCodexFile {
    path: PathBuf,
    size_bytes: u64,
    modified_at: String,
    sha256: String,
}

#[derive(Debug)]
struct RecordedCodexFile {
    id: i64,
    changed: bool,
}

#[derive(Debug)]
struct RawEventLine {
    line_number: i64,
    raw_json: String,
    line_sha256: String,
    event_type: Option<String>,
    event_time: Option<String>,
    parse_error: Option<String>,
}

fn discover_and_record_codex_files(conn: &mut Connection, root: &Path) -> Result<IngestSummary> {
    if !root.exists() {
        return Err(IngestError::MissingCodexRoot {
            path: root.to_path_buf(),
        })
        .context("failed to discover Codex JSONL files");
    }
    if !root.is_dir() {
        return Err(IngestError::InvalidCodexRoot {
            path: root.to_path_buf(),
        })
        .context("failed to discover Codex JSONL files");
    }

    let mut summary = IngestSummary::default();
    let mut files = Vec::new();
    discover_jsonl_files(root, &mut files, &mut summary.warnings);
    summary.discovered = files.len();

    let tx = conn
        .transaction()
        .context("failed to record discovered Codex files")?;
    for file in files {
        let recorded_file = record_codex_file(&tx, &file)?;
        if recorded_file.changed {
            summary.recorded += 1;
        } else {
            summary.skipped += 1;
        }
        record_raw_events(&tx, recorded_file.id, &file.path, &mut summary)?;
    }
    derive_sessions(&tx)?;
    derive_messages(&tx)?;
    derive_commands(&tx)?;
    derive_approvals(&tx)?;
    derive_file_changes(&tx)?;
    derive_verification_facts(&tx)?;
    derive_failure_facts(&tx)?;
    derive_failure_resolution_facts(&tx)?;
    rebuild_raw_events_fts(&tx)?;
    rebuild_messages_fts(&tx)?;
    rebuild_command_output_fts(&tx)?;
    tx.commit()
        .context("failed to record discovered Codex files")?;

    Ok(summary)
}

fn discover_jsonl_files(
    root: &Path,
    files: &mut Vec<DiscoveredCodexFile>,
    warnings: &mut Vec<String>,
) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            warnings.push(format!(
                "could not read directory {}: {error}",
                root.display()
            ));
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warnings.push(format!(
                    "could not read entry under {}: {error}",
                    root.display()
                ));
                continue;
            }
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                warnings.push(format!(
                    "could not inspect file type for {}: {error}",
                    path.display()
                ));
                continue;
            }
        };

        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            discover_jsonl_files(&path, files, warnings);
            continue;
        }
        if !file_type.is_file()
            || !path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
        {
            continue;
        }

        match discover_jsonl_file(&path) {
            Ok(file) => files.push(file),
            Err(error) => warnings.push(error),
        }
    }
}

fn discover_jsonl_file(path: &Path) -> Result<DiscoveredCodexFile, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("could not read metadata for {}: {error}", path.display()))?;
    let sha256 = hash_file_sha256(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    Ok(DiscoveredCodexFile {
        path: path.to_path_buf(),
        size_bytes: metadata.len(),
        modified_at,
        sha256,
    })
}

fn hash_file_sha256(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(hex_digest(hasher.finalize().as_slice()))
}

fn record_codex_file(conn: &Connection, file: &DiscoveredCodexFile) -> Result<RecordedCodexFile> {
    let path = file.path.to_string_lossy().to_string();
    let existing: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, sha256 FROM codex_files WHERE path = ?1",
            [&path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .context("failed to inspect existing Codex file metadata")?;

    if let Some((id, sha256)) = existing {
        if sha256 == file.sha256 {
            return Ok(RecordedCodexFile { id, changed: false });
        }
    }

    conn.execute(
        "INSERT INTO codex_files (path, size_bytes, modified_at, sha256, ingested_at)
         VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
         ON CONFLICT(path) DO UPDATE SET
           size_bytes = excluded.size_bytes,
           modified_at = excluded.modified_at,
           sha256 = excluded.sha256,
           ingested_at = CURRENT_TIMESTAMP",
        (
            &path,
            file.size_bytes as i64,
            &file.modified_at,
            &file.sha256,
        ),
    )
    .context("failed to record Codex file metadata")?;

    let id = conn
        .query_row(
            "SELECT id FROM codex_files WHERE path = ?1",
            [&path],
            |row| row.get(0),
        )
        .context("failed to inspect recorded Codex file metadata")?;

    Ok(RecordedCodexFile { id, changed: true })
}

fn record_raw_events(
    conn: &Connection,
    codex_file_id: i64,
    path: &Path,
    summary: &mut IngestSummary,
) -> Result<()> {
    let file = File::open(path)
        .with_context(|| format!("failed to read Codex JSONL file {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut line_number = 0;

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .with_context(|| format!("failed to read Codex JSONL file {}", path.display()))?;
        if bytes_read == 0 {
            break;
        }
        line_number += 1;
        let raw_json = line.trim_end_matches(['\r', '\n']).to_string();
        if raw_json.trim().is_empty() {
            summary.raw_events_skipped += 1;
            continue;
        }

        let event = parse_raw_event_line(line_number, raw_json);
        if event.parse_error.is_some() {
            summary.malformed_lines += 1;
        } else if event.event_type.is_none() {
            summary.unknown_event_shapes += 1;
        }
        if upsert_raw_event(conn, codex_file_id, &event)? {
            summary.raw_events_stored += 1;
        } else {
            summary.raw_events_skipped += 1;
        }
    }

    Ok(())
}

fn parse_raw_event_line(line_number: i64, raw_json: String) -> RawEventLine {
    let line_sha256 = sha256_hex(&raw_json);
    match serde_json::from_str::<Value>(&raw_json) {
        Ok(value) => RawEventLine {
            line_number,
            raw_json,
            line_sha256,
            event_type: string_field(&value, &["type", "event_type"]),
            event_time: string_field(&value, &["timestamp", "time", "created_at"]),
            parse_error: None,
        },
        Err(error) => RawEventLine {
            line_number,
            raw_json,
            line_sha256,
            event_type: None,
            event_time: None,
            parse_error: Some(error.to_string()),
        },
    }
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

fn upsert_raw_event(conn: &Connection, codex_file_id: i64, event: &RawEventLine) -> Result<bool> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT line_sha256 FROM raw_events WHERE codex_file_id = ?1 AND line_number = ?2",
            (codex_file_id, event.line_number),
            |row| row.get(0),
        )
        .optional()
        .context("failed to inspect existing raw event")?;
    if existing.as_deref() == Some(event.line_sha256.as_str()) {
        return Ok(false);
    }

    conn.execute(
        "INSERT INTO raw_events (
            codex_file_id,
            line_number,
            raw_json,
            line_sha256,
            event_type,
            event_time,
            parse_error,
            created_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP)
         ON CONFLICT(codex_file_id, line_number) DO UPDATE SET
            raw_json = excluded.raw_json,
            line_sha256 = excluded.line_sha256,
            event_type = excluded.event_type,
            event_time = excluded.event_time,
            parse_error = excluded.parse_error,
            created_at = CURRENT_TIMESTAMP",
        (
            codex_file_id,
            event.line_number,
            &event.raw_json,
            &event.line_sha256,
            event.event_type.as_deref(),
            event.event_time.as_deref(),
            event.parse_error.as_deref(),
        ),
    )
    .context("failed to record raw Codex event")?;

    Ok(true)
}

fn rebuild_raw_events_fts(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT INTO raw_events_fts(raw_events_fts) VALUES ('rebuild')",
        [],
    )
    .context("failed to rebuild raw event FTS index")?;
    Ok(())
}

fn rebuild_messages_fts(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT INTO messages_fts(messages_fts) VALUES ('rebuild')",
        [],
    )
    .context("failed to rebuild message FTS index")?;
    Ok(())
}

fn rebuild_command_output_fts(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT INTO command_output_fts(command_output_fts) VALUES ('rebuild')",
        [],
    )
    .context("failed to rebuild command output FTS index")?;
    Ok(())
}

#[derive(Debug, Default)]
struct DerivedSession {
    codex_session_id: String,
    workspace_path: Option<String>,
    model: Option<String>,
    title: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    event_count: i64,
}

fn derive_sessions(conn: &Connection) -> Result<()> {
    let mut sessions = collect_sessions(conn)?;
    for session in sessions.values_mut() {
        let session_id = upsert_session(conn, session)?;
        conn.execute(
            "UPDATE raw_events
             SET session_id = ?1
             WHERE parse_error IS NULL
               AND json_extract(raw_json, '$.session_id') = ?2",
            (session_id, &session.codex_session_id),
        )
        .context("failed to link raw events to derived session")?;
    }

    Ok(())
}

fn collect_sessions(conn: &Connection) -> Result<BTreeMap<String, DerivedSession>> {
    let mut stmt = conn
        .prepare(
            "SELECT raw_json, event_time
             FROM raw_events
             WHERE parse_error IS NULL
             ORDER BY codex_file_id, line_number",
        )
        .context("failed to derive sessions from raw events")?;
    let mut rows = stmt
        .query([])
        .context("failed to derive sessions from raw events")?;
    let mut sessions = BTreeMap::new();

    while let Some(row) = rows
        .next()
        .context("failed to derive sessions from raw events")?
    {
        let raw_json: String = row
            .get(0)
            .context("failed to derive sessions from raw events")?;
        let event_time: Option<String> = row
            .get(1)
            .context("failed to derive sessions from raw events")?;
        let value: Value =
            serde_json::from_str(&raw_json).context("failed to derive sessions from raw events")?;
        let Some(codex_session_id) = value.get("session_id").and_then(Value::as_str) else {
            continue;
        };
        let entry = sessions
            .entry(codex_session_id.to_string())
            .or_insert_with(|| DerivedSession {
                codex_session_id: codex_session_id.to_string(),
                ..DerivedSession::default()
            });
        entry.event_count += 1;
        if entry.started_at.is_none() {
            entry.started_at = event_time.clone();
        }
        if event_time.is_some() {
            entry.ended_at = event_time;
        }
        if value.get("type").and_then(Value::as_str) == Some("session_meta") {
            fill_session_metadata(entry, &value);
        }
    }

    Ok(sessions)
}

fn fill_session_metadata(session: &mut DerivedSession, value: &Value) {
    if session.workspace_path.is_none() {
        session.workspace_path = string_field(value, &["workspace_path"]);
    }
    if session.model.is_none() {
        session.model = string_field(value, &["model"]);
    }
    if session.title.is_none() {
        session.title = string_field(value, &["title", "summary"]);
    }
}

fn upsert_session(conn: &Connection, session: &DerivedSession) -> Result<i64> {
    conn.execute(
        "INSERT INTO sessions (
            codex_session_id,
            workspace_path,
            model,
            title,
            started_at,
            ended_at,
            event_count,
            parse_status
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'parsed')
         ON CONFLICT(codex_session_id) DO UPDATE SET
            workspace_path = excluded.workspace_path,
            model = excluded.model,
            title = excluded.title,
            started_at = excluded.started_at,
            ended_at = excluded.ended_at,
            event_count = excluded.event_count,
            parse_status = excluded.parse_status",
        (
            &session.codex_session_id,
            session.workspace_path.as_deref(),
            session.model.as_deref(),
            session.title.as_deref(),
            session.started_at.as_deref(),
            session.ended_at.as_deref(),
            session.event_count,
        ),
    )
    .context("failed to record derived session")?;

    conn.query_row(
        "SELECT id FROM sessions WHERE codex_session_id = ?1",
        [&session.codex_session_id],
        |row| row.get(0),
    )
    .context("failed to inspect derived session")
}

fn derive_messages(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM messages", [])
        .context("failed to refresh derived messages")?;

    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, raw_json, event_time
             FROM raw_events
             WHERE parse_error IS NULL
             ORDER BY codex_file_id, line_number",
        )
        .context("failed to derive messages from raw events")?;
    let mut rows = stmt
        .query([])
        .context("failed to derive messages from raw events")?;

    while let Some(row) = rows
        .next()
        .context("failed to derive messages from raw events")?
    {
        let raw_event_id: i64 = row
            .get(0)
            .context("failed to derive messages from raw events")?;
        let session_id: Option<i64> = row
            .get(1)
            .context("failed to derive messages from raw events")?;
        let raw_json: String = row
            .get(2)
            .context("failed to derive messages from raw events")?;
        let created_at: Option<String> = row
            .get(3)
            .context("failed to derive messages from raw events")?;
        let value: Value =
            serde_json::from_str(&raw_json).context("failed to derive messages from raw events")?;
        let Some((role, text)) = extract_message(&value) else {
            continue;
        };

        conn.execute(
            "INSERT INTO messages (raw_event_id, session_id, role, text, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                raw_event_id,
                session_id,
                role.as_str(),
                text.as_str(),
                created_at.as_deref(),
            ),
        )
        .context("failed to record derived message")?;
    }

    Ok(())
}

fn extract_message(value: &Value) -> Option<(String, String)> {
    if value.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }

    let message = value.get("message")?;
    let role = message.get("role").and_then(Value::as_str)?;
    if !matches!(role, "user" | "assistant") {
        return None;
    }

    let text = message.get("content").and_then(Value::as_str)?.trim();
    if text.is_empty() {
        return None;
    }

    Some((role.to_owned(), text.to_owned()))
}

#[derive(Debug)]
struct DerivedCommand {
    command: String,
    cwd: Option<String>,
    status: Option<String>,
    exit_code: Option<i64>,
    output: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
}

fn derive_commands(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM commands", [])
        .context("failed to refresh derived commands")?;

    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, raw_json, event_time
             FROM raw_events
             WHERE parse_error IS NULL
             ORDER BY codex_file_id, line_number",
        )
        .context("failed to derive commands from raw events")?;
    let mut rows = stmt
        .query([])
        .context("failed to derive commands from raw events")?;

    while let Some(row) = rows
        .next()
        .context("failed to derive commands from raw events")?
    {
        let raw_event_id: i64 = row
            .get(0)
            .context("failed to derive commands from raw events")?;
        let session_id: Option<i64> = row
            .get(1)
            .context("failed to derive commands from raw events")?;
        let raw_json: String = row
            .get(2)
            .context("failed to derive commands from raw events")?;
        let event_time: Option<String> = row
            .get(3)
            .context("failed to derive commands from raw events")?;
        let value: Value =
            serde_json::from_str(&raw_json).context("failed to derive commands from raw events")?;
        let Some(command) = extract_command(&value, event_time.as_deref()) else {
            continue;
        };

        conn.execute(
            "INSERT INTO commands (
                raw_event_id,
                session_id,
                command,
                cwd,
                status,
                exit_code,
                output,
                started_at,
                ended_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            (
                raw_event_id,
                session_id,
                command.command.as_str(),
                command.cwd.as_deref(),
                command.status.as_deref(),
                command.exit_code,
                command.output.as_deref(),
                command.started_at.as_deref(),
                command.ended_at.as_deref(),
            ),
        )
        .context("failed to record derived command")?;
    }

    Ok(())
}

fn extract_command(value: &Value, event_time: Option<&str>) -> Option<DerivedCommand> {
    if value.get("type").and_then(Value::as_str) != Some("command") {
        return None;
    }

    let command = value.get("command")?;
    let command_text = command.get("cmd").and_then(Value::as_str)?.trim();
    if command_text.is_empty() {
        return None;
    }

    let started_at = string_field(command, &["started_at", "start_time"])
        .or_else(|| event_time.map(ToOwned::to_owned));
    let ended_at = string_field(command, &["ended_at", "end_time"])
        .or_else(|| event_time.map(ToOwned::to_owned));

    Some(DerivedCommand {
        command: command_text.to_owned(),
        cwd: string_field(command, &["cwd"]),
        status: string_field(command, &["status"]),
        exit_code: command.get("exit_code").and_then(Value::as_i64),
        output: string_field(command, &["output"]),
        started_at,
        ended_at,
    })
}

fn derive_approvals(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM approvals", [])
        .context("failed to refresh derived approvals")?;

    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, raw_json, event_time
             FROM raw_events
             WHERE parse_error IS NULL
             ORDER BY codex_file_id, line_number",
        )
        .context("failed to derive approvals from raw events")?;
    let mut rows = stmt
        .query([])
        .context("failed to derive approvals from raw events")?;

    while let Some(row) = rows
        .next()
        .context("failed to derive approvals from raw events")?
    {
        let raw_event_id: i64 = row
            .get(0)
            .context("failed to derive approvals from raw events")?;
        let session_id: Option<i64> = row
            .get(1)
            .context("failed to derive approvals from raw events")?;
        let raw_json: String = row
            .get(2)
            .context("failed to derive approvals from raw events")?;
        let created_at: Option<String> = row
            .get(3)
            .context("failed to derive approvals from raw events")?;
        let value: Value = serde_json::from_str(&raw_json)
            .context("failed to derive approvals from raw events")?;
        let Some(approval) = extract_approval(&value) else {
            continue;
        };

        conn.execute(
            "INSERT INTO approvals (
                raw_event_id,
                session_id,
                action,
                decision,
                sandbox_mode,
                command,
                created_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            (
                raw_event_id,
                session_id,
                approval.action.as_deref(),
                approval.decision.as_deref(),
                approval.sandbox_mode.as_deref(),
                approval.command.as_deref(),
                created_at.as_deref(),
            ),
        )
        .context("failed to record derived approval")?;
    }

    Ok(())
}

#[derive(Debug)]
struct DerivedApproval {
    action: Option<String>,
    decision: Option<String>,
    sandbox_mode: Option<String>,
    command: Option<String>,
}

fn extract_approval(value: &Value) -> Option<DerivedApproval> {
    if value.get("type").and_then(Value::as_str) != Some("approval") {
        return None;
    }

    let approval = value.get("approval")?;
    Some(DerivedApproval {
        action: string_field(approval, &["requested_action", "action"]),
        decision: string_field(approval, &["decision"]),
        sandbox_mode: string_field(approval, &["sandbox_mode"]),
        command: string_field(approval, &["command"]),
    })
}

fn derive_file_changes(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM file_changes", [])
        .context("failed to refresh derived file changes")?;

    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, raw_json
             FROM raw_events
             WHERE parse_error IS NULL
             ORDER BY codex_file_id, line_number",
        )
        .context("failed to derive file changes from raw events")?;
    let mut rows = stmt
        .query([])
        .context("failed to derive file changes from raw events")?;

    while let Some(row) = rows
        .next()
        .context("failed to derive file changes from raw events")?
    {
        let raw_event_id: i64 = row
            .get(0)
            .context("failed to derive file changes from raw events")?;
        let session_id: Option<i64> = row
            .get(1)
            .context("failed to derive file changes from raw events")?;
        let raw_json: String = row
            .get(2)
            .context("failed to derive file changes from raw events")?;
        let value: Value = serde_json::from_str(&raw_json)
            .context("failed to derive file changes from raw events")?;
        let Some(file_change) = extract_file_change(&value) else {
            continue;
        };

        conn.execute(
            "INSERT INTO file_changes (
                raw_event_id,
                session_id,
                path,
                change_type,
                diff_text,
                lines_added,
                lines_deleted
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            (
                raw_event_id,
                session_id,
                file_change.path.as_str(),
                file_change.change_type.as_deref(),
                file_change.diff_text.as_deref(),
                file_change.lines_added,
                file_change.lines_deleted,
            ),
        )
        .context("failed to record derived file change")?;
    }

    Ok(())
}

#[derive(Debug)]
struct DerivedFileChange {
    path: String,
    change_type: Option<String>,
    diff_text: Option<String>,
    lines_added: Option<i64>,
    lines_deleted: Option<i64>,
}

fn extract_file_change(value: &Value) -> Option<DerivedFileChange> {
    if value.get("type").and_then(Value::as_str) != Some("file_change") {
        return None;
    }

    let file_change = value.get("file_change")?;
    let path = file_change.get("path").and_then(Value::as_str)?.trim();
    if path.is_empty() {
        return None;
    }

    Some(DerivedFileChange {
        path: path.to_owned(),
        change_type: string_field(file_change, &["change_type"]),
        diff_text: string_field(file_change, &["diff", "diff_text"]),
        lines_added: file_change.get("lines_added").and_then(Value::as_i64),
        lines_deleted: file_change.get("lines_deleted").and_then(Value::as_i64),
    })
}

fn derive_verification_facts(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM proof_facts WHERE kind = 'verification'", [])
        .context("failed to refresh verification proof facts")?;

    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, command, status, exit_code
             FROM commands
             ORDER BY id",
        )
        .context("failed to derive verification proof facts")?;
    let mut rows = stmt
        .query([])
        .context("failed to derive verification proof facts")?;

    while let Some(row) = rows
        .next()
        .context("failed to derive verification proof facts")?
    {
        let command_id: i64 = row
            .get(0)
            .context("failed to derive verification proof facts")?;
        let session_id: Option<i64> = row
            .get(1)
            .context("failed to derive verification proof facts")?;
        let command: String = row
            .get(2)
            .context("failed to derive verification proof facts")?;
        let status: Option<String> = row
            .get(3)
            .context("failed to derive verification proof facts")?;
        let exit_code: Option<i64> = row
            .get(4)
            .context("failed to derive verification proof facts")?;

        let Some(detector) = classify_verification_command(&command) else {
            continue;
        };
        let proof_status = verification_status(status.as_deref(), exit_code);
        let reason = format!("detector={detector}; confidence=high");

        conn.execute(
            "INSERT INTO proof_facts (
                session_id,
                command_id,
                kind,
                subject,
                status,
                reason
             )
             VALUES (?1, ?2, 'verification', ?3, ?4, ?5)",
            (
                session_id,
                command_id,
                command.as_str(),
                proof_status,
                reason.as_str(),
            ),
        )
        .context("failed to record verification proof fact")?;
    }

    Ok(())
}

fn classify_verification_command(command: &str) -> Option<&'static str> {
    const DETECTORS: &[(&str, &str)] = &[
        ("cargo test", "cargo test"),
        ("cargo build", "cargo build"),
        ("cargo clippy", "cargo clippy"),
        ("go test", "go test"),
        ("go build", "go build"),
        ("golangci-lint", "golangci-lint"),
        ("pytest", "pytest"),
        ("ruff", "ruff"),
        ("npm test", "npm test"),
        ("npm run build", "npm run build"),
        ("npm run lint", "npm run lint"),
        ("npm run typecheck", "npm run typecheck"),
        ("pnpm test", "pnpm test"),
        ("pnpm build", "pnpm build"),
        ("pnpm lint", "pnpm lint"),
        ("pnpm typecheck", "pnpm typecheck"),
        ("make test", "make test"),
        ("make build", "make build"),
        ("tsc", "tsc"),
        ("eslint", "eslint"),
    ];

    let normalized = command.trim().to_ascii_lowercase();
    DETECTORS
        .iter()
        .find(|(prefix, _)| command_has_prefix(&normalized, prefix))
        .map(|(_, detector)| *detector)
}

fn command_has_prefix(command: &str, prefix: &str) -> bool {
    command == prefix
        || command
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

fn verification_status(status: Option<&str>, exit_code: Option<i64>) -> &'static str {
    match exit_code {
        Some(0) => return "passed",
        Some(_) => return "failed",
        None => {}
    }

    match status.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if matches!(value.as_str(), "success" | "passed" | "pass" | "ok") => "passed",
        Some(value) if matches!(value.as_str(), "failure" | "failed" | "fail" | "error") => {
            "failed"
        }
        _ => "unknown",
    }
}

fn derive_failure_facts(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM proof_facts WHERE kind = 'failure'", [])
        .context("failed to refresh failure proof facts")?;

    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, command, status, exit_code, output
             FROM commands
             ORDER BY id",
        )
        .context("failed to derive failure proof facts")?;
    let mut rows = stmt
        .query([])
        .context("failed to derive failure proof facts")?;

    while let Some(row) = rows
        .next()
        .context("failed to derive failure proof facts")?
    {
        let command_id: i64 = row.get(0).context("failed to derive failure proof facts")?;
        let session_id: Option<i64> = row.get(1).context("failed to derive failure proof facts")?;
        let command: String = row.get(2).context("failed to derive failure proof facts")?;
        let status: Option<String> = row.get(3).context("failed to derive failure proof facts")?;
        let exit_code: Option<i64> = row.get(4).context("failed to derive failure proof facts")?;
        let output: Option<String> = row.get(5).context("failed to derive failure proof facts")?;

        let Some(reason) = failure_reason(status.as_deref(), exit_code, output.as_deref()) else {
            continue;
        };

        conn.execute(
            "INSERT INTO proof_facts (
                session_id,
                command_id,
                kind,
                subject,
                status,
                reason
             )
             VALUES (?1, ?2, 'failure', ?3, 'failed', ?4)",
            (session_id, command_id, command.as_str(), reason.as_str()),
        )
        .context("failed to record failure proof fact")?;
    }

    Ok(())
}

fn failure_reason(
    status: Option<&str>,
    exit_code: Option<i64>,
    output: Option<&str>,
) -> Option<String> {
    match exit_code {
        Some(0) => return None,
        Some(code) => return Some(format!("signal=exit-code; exit_code={code}")),
        None => {}
    }

    if let Some(status) = status {
        let normalized = status.trim().to_ascii_lowercase();
        if matches!(normalized.as_str(), "failure" | "failed" | "fail" | "error") {
            return Some(format!("signal=status; status={normalized}"));
        }
        if matches!(normalized.as_str(), "success" | "passed" | "pass" | "ok") {
            return None;
        }
    }

    let output = output?.to_ascii_lowercase();
    failure_output_token(&output).map(|token| format!("signal=output-token; token={token}"))
}

fn failure_output_token(output: &str) -> Option<&'static str> {
    const TOKENS: &[&str] = &[
        "permission denied",
        "command not found",
        "no such file",
        "timed out",
        "sandbox",
        "network",
        "failed",
        "error",
    ];

    TOKENS.iter().copied().find(|token| output.contains(token))
}

#[derive(Debug)]
struct CommandResolutionEvidence {
    id: i64,
    session_id: Option<i64>,
    command: String,
    detector: Option<&'static str>,
    verification_status: &'static str,
    has_failure: bool,
}

fn derive_failure_resolution_facts(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM proof_facts WHERE kind = 'failure_resolution'",
        [],
    )
    .context("failed to refresh failure resolution proof facts")?;

    let commands = load_command_resolution_evidence(conn)?;
    for failed in commands
        .iter()
        .filter(|command| command.detector.is_some() && command.has_failure)
    {
        let detector = failed.detector.expect("detector checked above");
        let later_passes = commands
            .iter()
            .filter(|candidate| {
                candidate.id > failed.id
                    && candidate.detector == Some(detector)
                    && candidate.verification_status == "passed"
            })
            .collect::<Vec<_>>();

        let (status, reason) = resolve_failed_command(failed, detector, &later_passes);
        conn.execute(
            "INSERT INTO proof_facts (
                session_id,
                command_id,
                kind,
                subject,
                status,
                reason
             )
             VALUES (?1, ?2, 'failure_resolution', ?3, ?4, ?5)",
            (
                failed.session_id,
                failed.id,
                failed.command.as_str(),
                status,
                reason.as_str(),
            ),
        )
        .context("failed to record failure resolution proof fact")?;
    }

    Ok(())
}

fn load_command_resolution_evidence(conn: &Connection) -> Result<Vec<CommandResolutionEvidence>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, command, status, exit_code, output
             FROM commands
             ORDER BY id",
        )
        .context("failed to derive failure resolution proof facts")?;
    let mut rows = stmt
        .query([])
        .context("failed to derive failure resolution proof facts")?;
    let mut commands = Vec::new();

    while let Some(row) = rows
        .next()
        .context("failed to derive failure resolution proof facts")?
    {
        let id: i64 = row
            .get(0)
            .context("failed to derive failure resolution proof facts")?;
        let session_id: Option<i64> = row
            .get(1)
            .context("failed to derive failure resolution proof facts")?;
        let command: String = row
            .get(2)
            .context("failed to derive failure resolution proof facts")?;
        let status: Option<String> = row
            .get(3)
            .context("failed to derive failure resolution proof facts")?;
        let exit_code: Option<i64> = row
            .get(4)
            .context("failed to derive failure resolution proof facts")?;
        let output: Option<String> = row
            .get(5)
            .context("failed to derive failure resolution proof facts")?;

        commands.push(CommandResolutionEvidence {
            id,
            session_id,
            detector: classify_verification_command(&command),
            verification_status: verification_status(status.as_deref(), exit_code),
            has_failure: failure_reason(status.as_deref(), exit_code, output.as_deref()).is_some(),
            command,
        });
    }

    Ok(commands)
}

fn resolve_failed_command(
    failed: &CommandResolutionEvidence,
    detector: &str,
    later_passes: &[&CommandResolutionEvidence],
) -> (&'static str, String) {
    if later_passes.is_empty() {
        return (
            "unresolved",
            format!("resolution=no-later-pass; detector={detector}"),
        );
    }

    if let Some(exact) = later_passes.iter().find(|candidate| {
        normalized_command(&candidate.command) == normalized_command(&failed.command)
    }) {
        return (
            "resolved",
            format!(
                "resolution=exact-rerun; detector={detector}; matched_command_id={}",
                exact.id
            ),
        );
    }

    if let Some(compatible) = later_passes
        .iter()
        .find(|candidate| commands_are_compatible(&failed.command, &candidate.command, detector))
    {
        return (
            "resolved",
            format!(
                "resolution=compatible-rerun; detector={detector}; matched_command_id={}",
                compatible.id
            ),
        );
    }

    (
        "unknown",
        format!("resolution=ambiguous; detector={detector}"),
    )
}

fn commands_are_compatible(failed: &str, passed: &str, detector: &str) -> bool {
    let failed = normalized_command(failed);
    let passed = normalized_command(passed);
    if failed == passed {
        return true;
    }

    let failed_rest = command_remainder_after_detector(&failed, detector);
    let passed_rest = command_remainder_after_detector(&passed, detector);

    failed_rest.is_some_and(|rest| !rest.is_empty()) && passed_rest == Some("")
}

fn command_remainder_after_detector<'a>(command: &'a str, detector: &str) -> Option<&'a str> {
    if command == detector {
        return Some("");
    }
    command
        .strip_prefix(detector)
        .and_then(|rest| rest.strip_prefix(char::is_whitespace))
        .map(str::trim)
}

fn normalized_command(command: &str) -> String {
    command
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn sha256_hex(text: &str) -> String {
    hex_digest(Sha256::digest(text.as_bytes()).as_slice())
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn inspect_codex(root: &Path) -> CodexStatus {
    if !root.exists() {
        return CodexStatus {
            root: root.to_path_buf(),
            root_state: "missing",
            jsonl_files: 0,
            warnings: vec![format!(
                "Codex root does not exist: {}; run with --codex-root <path> if your history lives elsewhere",
                root.display()
            )],
        };
    }

    if !root.is_dir() {
        return CodexStatus {
            root: root.to_path_buf(),
            root_state: "invalid",
            jsonl_files: 0,
            warnings: vec![format!("Codex root is not a directory: {}", root.display())],
        };
    }

    let mut warnings = Vec::new();
    let jsonl_files = count_jsonl_files(root, &mut warnings);
    if jsonl_files == 0 {
        warnings.push(format!(
            "No Codex JSONL files found under {}",
            root.display()
        ));
    }

    CodexStatus {
        root: root.to_path_buf(),
        root_state: "ok",
        jsonl_files,
        warnings,
    }
}

fn count_jsonl_files(root: &Path, warnings: &mut Vec<String>) -> usize {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            warnings.push(format!(
                "Could not read Codex directory {}: {error}",
                root.display()
            ));
            return 0;
        }
    };

    let mut count = 0;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warnings.push(format!(
                    "Could not read an entry under {}: {error}",
                    root.display()
                ));
                continue;
            }
        };
        let path = entry.path();
        if path.is_dir() {
            count += count_jsonl_files(&path, warnings);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
        {
            count += 1;
        }
    }
    count
}

fn inspect_git() -> GitStatus {
    let repo_root = run_git(["rev-parse", "--show-toplevel"]);
    let Some(repo_root) = repo_root else {
        return GitStatus {
            repo_root: None,
            branch: None,
            warnings: vec!["not inside a git repo; git context will be unavailable".to_string()],
        };
    };

    let branch = run_git(["branch", "--show-current"])
        .filter(|branch| !branch.is_empty())
        .or_else(|| run_git(["rev-parse", "--short", "HEAD"]).map(|head| format!("HEAD {head}")));

    GitStatus {
        repo_root: Some(PathBuf::from(repo_root)),
        branch,
        warnings: Vec::new(),
    }
}

fn inspect_proof_git(repo_path: &Path, since: &str) -> Result<ProofGitContext> {
    let repo_root = run_git_in(repo_path, ["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .map_err(|_| GitError::NotRepository {
            path: repo_path.to_path_buf(),
        })
        .with_context(|| {
            format!(
                "not a git repository: {}; pass --repo <PATH> to choose a repository",
                repo_path.display()
            )
        })?;
    let head = run_git_in(&repo_root, ["rev-parse", "HEAD"])
        .map_err(|_| GitError::MissingHead {
            path: repo_root.clone(),
        })
        .with_context(|| format!("failed to inspect git HEAD in {}", repo_root.display()))?;
    let branch = run_git_in(&repo_root, ["branch", "--show-current"])
        .ok()
        .filter(|branch| !branch.is_empty())
        .unwrap_or_else(|| format!("HEAD {}", short_hash(&head)));
    let merge_base = run_git_in(&repo_root, ["merge-base", since, "HEAD"])
        .map_err(|_| GitError::InvalidBaseRef {
            reference: since.to_string(),
        })
        .with_context(|| {
            format!(
                "invalid git base ref `{since}` or no merge base with HEAD in {}",
                repo_root.display()
            )
        })?;
    let dirty = !run_git_in(&repo_root, ["status", "--porcelain"])
        .map_err(|_| GitError::StatusFailed {
            path: repo_root.clone(),
        })
        .with_context(|| format!("failed to inspect git status in {}", repo_root.display()))?
        .is_empty();

    Ok(ProofGitContext {
        repo_root,
        branch,
        head,
        merge_base,
        dirty,
    })
}

fn inspect_changed_files(repo_root: &Path, merge_base: &str) -> Result<ChangedFiles> {
    let numstat = run_git_in(repo_root, ["diff", "--numstat", "-M", merge_base, "HEAD"])
        .map_err(|_| GitError::DiffFailed {
            path: repo_root.to_path_buf(),
        })
        .with_context(|| format!("failed to inspect changed files in {}", repo_root.display()))?;
    let name_status = run_git_in(
        repo_root,
        ["diff", "--name-status", "-M", merge_base, "HEAD"],
    )
    .map_err(|_| GitError::DiffFailed {
        path: repo_root.to_path_buf(),
    })
    .with_context(|| format!("failed to inspect changed files in {}", repo_root.display()))?;

    let mut stats = parse_numstat(&numstat);
    let mut files = parse_name_status(&name_status);
    for file in &mut files {
        if let Some((additions, deletions)) = stats.remove(&file.path) {
            file.additions = additions;
            file.deletions = deletions;
        }
    }
    files.sort_by_key(|file| file.display_path());
    let total_additions = files.iter().filter_map(|file| file.additions).sum();
    let total_deletions = files.iter().filter_map(|file| file.deletions).sum();
    let docs_only = !files.is_empty() && files.iter().all(|file| is_docs_path(&file.path));

    Ok(ChangedFiles {
        files,
        total_additions,
        total_deletions,
        docs_only,
    })
}

fn parse_numstat(text: &str) -> BTreeMap<String, (Option<i64>, Option<i64>)> {
    let mut stats = BTreeMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.split('\t');
        let Some(additions) = fields.next() else {
            continue;
        };
        let Some(deletions) = fields.next() else {
            continue;
        };
        let Some(path) = fields.next_back() else {
            continue;
        };
        stats.insert(
            normalize_numstat_path(path),
            (parse_diff_stat(additions), parse_diff_stat(deletions)),
        );
    }
    stats
}

fn normalize_numstat_path(path: &str) -> String {
    if let Some((_, new_path)) = path.split_once(" => ") {
        return new_path.replace(['{', '}'], "");
    }
    path.to_string()
}

fn parse_name_status(text: &str) -> Vec<ChangedFile> {
    let mut files = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.split('\t');
        let Some(status) = fields.next() else {
            continue;
        };
        let short_status = status.chars().next().unwrap_or('?').to_string();
        if short_status == "R" {
            let Some(previous_path) = fields.next() else {
                continue;
            };
            let Some(path) = fields.next() else {
                continue;
            };
            files.push(ChangedFile {
                status: short_status,
                path: path.to_string(),
                previous_path: Some(previous_path.to_string()),
                additions: None,
                deletions: None,
            });
        } else if let Some(path) = fields.next() {
            files.push(ChangedFile {
                status: short_status,
                path: path.to_string(),
                previous_path: None,
                additions: None,
                deletions: None,
            });
        }
    }
    files
}

fn parse_diff_stat(text: &str) -> Option<i64> {
    text.parse().ok()
}

fn display_stat(value: Option<i64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn is_docs_path(path: &str) -> bool {
    path == "README.md" || path == "AGENTS.md" || path.starts_with("docs/") || path.ends_with(".md")
}

fn classify_changed_risks(changed: &ChangedFiles) -> RiskReport {
    if changed.docs_only {
        return RiskReport {
            level: "low",
            risky_file_count: 0,
            findings: Vec::new(),
        };
    }

    let mut risky_files = BTreeSet::new();
    let mut findings = Vec::new();
    for file in &changed.files {
        if is_docs_path(&file.path) {
            continue;
        }
        let display_path = file.display_path();
        let paths = file.risk_paths();
        for rule in RISK_RULES {
            if paths.iter().any(|path| rule.matches(path)) {
                risky_files.insert(display_path.clone());
                findings.push(RiskFinding {
                    category: rule.category,
                    path: display_path.clone(),
                    reason: rule.reason,
                });
            }
        }
    }

    RiskReport {
        level: if risky_files.is_empty() {
            "low"
        } else {
            "elevated"
        },
        risky_file_count: risky_files.len(),
        findings,
    }
}

impl ChangedFile {
    fn risk_paths(&self) -> Vec<&str> {
        let mut paths = vec![self.path.as_str()];
        if let Some(previous_path) = &self.previous_path {
            paths.push(previous_path.as_str());
        }
        paths
    }
}

struct RiskRule {
    category: &'static str,
    reason: &'static str,
    matches: fn(&str) -> bool,
}

impl RiskRule {
    fn matches(&self, path: &str) -> bool {
        (self.matches)(path)
    }
}

const RISK_RULES: &[RiskRule] = &[
    RiskRule {
        category: "auth",
        reason: "authentication path",
        matches: risk_auth,
    },
    RiskRule {
        category: "identity",
        reason: "identity path",
        matches: risk_identity,
    },
    RiskRule {
        category: "security",
        reason: "security path",
        matches: risk_security,
    },
    RiskRule {
        category: "secrets",
        reason: "secret-like path",
        matches: risk_secrets,
    },
    RiskRule {
        category: "config",
        reason: "configuration path",
        matches: risk_config,
    },
    RiskRule {
        category: "infra",
        reason: "infrastructure path",
        matches: risk_infra,
    },
    RiskRule {
        category: "migration",
        reason: "migration path",
        matches: risk_migration,
    },
    RiskRule {
        category: "CI/CD",
        reason: "automation workflow path",
        matches: risk_cicd,
    },
    RiskRule {
        category: "production",
        reason: "production path",
        matches: risk_production,
    },
    RiskRule {
        category: "Kubernetes",
        reason: "Kubernetes path",
        matches: risk_kubernetes,
    },
    RiskRule {
        category: "Terraform",
        reason: "Terraform path",
        matches: risk_terraform,
    },
    RiskRule {
        category: "database",
        reason: "database path",
        matches: risk_database,
    },
    RiskRule {
        category: "release",
        reason: "release path",
        matches: risk_release,
    },
];

fn risk_auth(path: &str) -> bool {
    path_has_segment(path, "auth") || path_contains_any(path, &["oauth", "login"])
}

fn risk_identity(path: &str) -> bool {
    path_has_segment(path, "identity") || path_contains_any(path, &["iam", "oidc", "sso"])
}

fn risk_security(path: &str) -> bool {
    path_has_segment(path, "security") || path_contains_any(path, &["rbac", "policy"])
}

fn risk_secrets(path: &str) -> bool {
    let lower = normalized_path(path);
    lower == ".env"
        || lower.ends_with("/.env")
        || path_contains_any(path, &["secret", "secrets", "credential", "private_key"])
}

fn risk_config(path: &str) -> bool {
    let lower = normalized_path(path);
    path_contains_any(path, &["config", "settings"])
        || lower == ".env"
        || lower.ends_with("/.env")
        || lower == ".cargo/config.toml"
}

fn risk_infra(path: &str) -> bool {
    path_has_segment(path, "infra")
        || path_has_segment(path, "infrastructure")
        || path_contains_any(
            path,
            &[
                "dockerfile",
                "docker-compose",
                ".github/workflows",
                "terraform/",
                "k8s/",
                "helm/",
                "charts/",
            ],
        )
}

fn risk_migration(path: &str) -> bool {
    path_contains_any(path, &["migration", "migrations"])
}

fn risk_cicd(path: &str) -> bool {
    path_contains_any(
        path,
        &[
            ".github/workflows",
            ".gitlab-ci",
            "azure-pipelines",
            "jenkinsfile",
        ],
    )
}

fn risk_production(path: &str) -> bool {
    path_has_segment(path, "prod")
        || path_has_segment(path, "production")
        || path_contains_any(path, &["-prod", "production"])
}

fn risk_kubernetes(path: &str) -> bool {
    path_contains_any(
        path,
        &["k8s/", "kubernetes", "helm/", "charts/", "deployment.yaml"],
    )
}

fn risk_terraform(path: &str) -> bool {
    let lower = normalized_path(path);
    lower.ends_with(".tf") || path_contains_any(path, &["terraform/"])
}

fn risk_database(path: &str) -> bool {
    let lower = normalized_path(path);
    path_has_segment(path, "db")
        || path_has_segment(path, "database")
        || path_contains_any(path, &["migrations", "schema"])
        || lower.ends_with(".sql")
}

fn risk_release(path: &str) -> bool {
    path_has_segment(path, "release")
        || path_has_segment(path, "releases")
        || path_contains_any(path, &["release"])
}

fn path_has_segment(path: &str, segment: &str) -> bool {
    normalized_path(path).split('/').any(|part| part == segment)
}

fn path_contains_any(path: &str, needles: &[&str]) -> bool {
    let lower = normalized_path(path);
    needles.iter().any(|needle| lower.contains(needle))
}

fn normalized_path(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn decide_proof(
    db_path: &Path,
    changed: &ChangedFiles,
    correlation: &SessionCorrelation,
) -> Result<ProofDecision> {
    if changed.files.is_empty() {
        return Ok(unknown_decision("no changed files for selected git scope"));
    }
    if !db_path.exists() {
        return Ok(unknown_decision("local proof database is missing"));
    }

    let conn = open_existing_database(db_path)?;
    let relevant_sessions = correlated_sessions_by_id(&correlation.relevant);
    let ambiguous_sessions = correlated_sessions_by_id(&correlation.ambiguous);

    if relevant_sessions.is_empty() {
        let ambiguous_facts = load_decision_facts(&conn, &ambiguous_sessions)?;
        if ambiguous_facts.iter().any(is_decision_evidence) {
            return Ok(unknown_decision(
                "only ambiguous verification evidence found",
            ));
        }
        return Ok(unknown_decision("no relevant Codex sessions"));
    }

    let facts = load_decision_facts(&conn, &relevant_sessions)?;
    let verification_facts = facts
        .iter()
        .filter(|fact| fact.kind == "verification")
        .collect::<Vec<_>>();
    if verification_facts.is_empty() {
        return Ok(unknown_decision("no relevant verification evidence"));
    }

    let resolution_by_command = facts
        .iter()
        .filter(|fact| fact.kind == "failure_resolution")
        .filter_map(|fact| fact.command_id.map(|command_id| (command_id, fact)))
        .collect::<BTreeMap<_, _>>();

    let mut not_ready_reasons = Vec::new();
    for fact in facts
        .iter()
        .filter(|fact| fact.kind == "failure_resolution" && fact.status == "unresolved")
    {
        not_ready_reasons.push(format!(
            "unresolved verification failure: {} {}",
            fact.codex_session_id,
            fact.subject.as_deref().unwrap_or("(unknown verification)")
        ));
    }
    for fact in verification_facts
        .iter()
        .filter(|fact| fact.status == "failed")
    {
        let resolved = fact
            .command_id
            .and_then(|command_id| resolution_by_command.get(&command_id))
            .is_some_and(|resolution| resolution.status == "resolved");
        if !resolved {
            not_ready_reasons.push(format!(
                "unresolved verification failure: {} {}",
                fact.codex_session_id,
                fact.subject.as_deref().unwrap_or("(unknown verification)")
            ));
        }
    }
    sort_dedup(&mut not_ready_reasons);
    if !not_ready_reasons.is_empty() {
        return Ok(ProofDecision {
            status: "NOT READY",
            reasons: not_ready_reasons,
        });
    }

    let mut unknown_reasons = Vec::new();
    for fact in facts
        .iter()
        .filter(|fact| fact.kind == "failure_resolution" && fact.status == "unknown")
    {
        unknown_reasons.push(format!(
            "ambiguous failure resolution: {} {}",
            fact.codex_session_id,
            fact.subject.as_deref().unwrap_or("(unknown verification)")
        ));
    }
    if verification_facts
        .iter()
        .all(|fact| fact.status == "unknown")
    {
        unknown_reasons.push("only unknown verification evidence".to_string());
    } else if !verification_facts
        .iter()
        .any(|fact| fact.status == "passed")
    {
        unknown_reasons.push("no passing verification evidence".to_string());
    }
    sort_dedup(&mut unknown_reasons);
    if !unknown_reasons.is_empty() {
        return Ok(ProofDecision {
            status: "UNKNOWN",
            reasons: unknown_reasons,
        });
    }

    let mut ready_reasons = facts
        .iter()
        .filter(|fact| fact.kind == "failure_resolution" && fact.status == "resolved")
        .map(|fact| {
            format!(
                "resolved verification failure: {} {}",
                fact.codex_session_id,
                fact.subject.as_deref().unwrap_or("(unknown verification)")
            )
        })
        .collect::<Vec<_>>();
    ready_reasons.extend(
        verification_facts
            .iter()
            .filter(|fact| fact.status == "passed")
            .map(|fact| {
                format!(
                    "relevant verification passed: {} {}",
                    fact.codex_session_id,
                    fact.subject.as_deref().unwrap_or("(unknown verification)")
                )
            }),
    );
    sort_dedup(&mut ready_reasons);

    Ok(ProofDecision {
        status: "READY",
        reasons: ready_reasons,
    })
}

fn unknown_decision(reason: &str) -> ProofDecision {
    ProofDecision {
        status: "UNKNOWN",
        reasons: vec![reason.to_string()],
    }
}

fn sort_dedup(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

fn is_decision_evidence(fact: &DecisionFact) -> bool {
    matches!(fact.kind.as_str(), "verification" | "failure_resolution")
}

fn load_decision_facts(
    conn: &Connection,
    sessions: &BTreeMap<i64, &CorrelatedSession>,
) -> Result<Vec<DecisionFact>> {
    let mut facts = Vec::new();
    for session in sessions.values() {
        let mut stmt = conn
            .prepare(
                "SELECT command_id, kind, subject, status
                 FROM proof_facts
                 WHERE session_id = ?1
                   AND kind IN ('verification', 'failure_resolution')
                 ORDER BY id",
            )
            .context("failed to load proof decision facts")?;
        let session_facts = stmt
            .query_map([session.id], |row| {
                Ok(DecisionFact {
                    session_id: session.id,
                    codex_session_id: session.codex_session_id.clone(),
                    command_id: row.get(0)?,
                    kind: row.get(1)?,
                    subject: row.get(2)?,
                    status: row.get(3)?,
                })
            })
            .context("failed to load proof decision facts")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("failed to load proof decision facts")?;
        facts.extend(session_facts);
    }

    facts.sort_by(|left, right| {
        (
            left.session_id,
            left.command_id,
            left.kind.as_str(),
            left.status.as_str(),
            left.subject.as_deref().unwrap_or(""),
        )
            .cmp(&(
                right.session_id,
                right.command_id,
                right.kind.as_str(),
                right.status.as_str(),
                right.subject.as_deref().unwrap_or(""),
            ))
    });
    Ok(facts)
}

fn classify_risky_commands(
    db_path: &Path,
    correlation: &SessionCorrelation,
) -> Result<RiskyCommandReport> {
    if !db_path.exists() {
        return Ok(RiskyCommandReport::default());
    }

    let conn = open_existing_database(db_path)?;
    let relevant_sessions = correlated_sessions_by_id(&correlation.relevant);
    let ambiguous_sessions = correlated_sessions_by_id(&correlation.ambiguous);

    Ok(RiskyCommandReport {
        relevant: load_risky_command_findings(&conn, &relevant_sessions)?,
        ambiguous: load_risky_command_findings(&conn, &ambiguous_sessions)?,
    })
}

fn correlated_sessions_by_id(sessions: &[CorrelatedSession]) -> BTreeMap<i64, &CorrelatedSession> {
    sessions
        .iter()
        .map(|session| (session.id, session))
        .collect()
}

fn load_risky_command_findings(
    conn: &Connection,
    sessions: &BTreeMap<i64, &CorrelatedSession>,
) -> Result<Vec<RiskyCommandFinding>> {
    let mut findings = Vec::new();
    for session in sessions.values() {
        let mut stmt = conn
            .prepare(
                "SELECT command
                 FROM commands
                 WHERE session_id = ?1
                 ORDER BY id",
            )
            .context("failed to load risky commands")?;
        let commands = stmt
            .query_map([session.id], |row| row.get::<_, String>(0))
            .context("failed to load risky commands")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("failed to load risky commands")?;

        for command in commands {
            let Some(classification) = classify_risky_command(&command) else {
                continue;
            };
            findings.push(RiskyCommandFinding {
                codex_session_id: session.codex_session_id.clone(),
                session_title: session.title.clone(),
                family: classification.family,
                severity: classification.severity,
                reason: classification.reason,
                command,
            });
        }
    }

    findings.sort_by(|left, right| {
        (
            left.codex_session_id.as_str(),
            left.command.as_str(),
            left.family,
        )
            .cmp(&(
                right.codex_session_id.as_str(),
                right.command.as_str(),
                right.family,
            ))
    });
    Ok(findings)
}

struct RiskyCommandClassification {
    family: &'static str,
    severity: &'static str,
    reason: &'static str,
}

fn classify_risky_command(command: &str) -> Option<RiskyCommandClassification> {
    let family = risky_command_family(command)?;
    let high = risky_command_is_high_severity(command, family);
    Some(RiskyCommandClassification {
        family,
        severity: if high { "high" } else { "elevated" },
        reason: if high {
            "production/destructive arguments"
        } else {
            "risky command family"
        },
    })
}

fn risky_command_family(command: &str) -> Option<&'static str> {
    let first = command.split_whitespace().next()?.to_ascii_lowercase();
    match first.as_str() {
        "aws" => Some("aws"),
        "kubectl" => Some("kubectl"),
        "terraform" => Some("terraform"),
        "helm" => Some("helm"),
        "docker" => Some("docker"),
        "gh" => Some("gh"),
        "rm" => Some("rm"),
        "chmod" => Some("chmod"),
        "chown" => Some("chown"),
        "curl" => Some("curl"),
        "scp" => Some("scp"),
        "ssh" => Some("ssh"),
        _ => None,
    }
}

fn risky_command_is_high_severity(command: &str, family: &str) -> bool {
    let normalized = command.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = normalized.to_ascii_lowercase();
    if lower.contains("prod")
        || lower.contains("production")
        || lower.contains("--force")
        || lower.contains(" destroy")
        || lower.contains(" delete")
        || lower.contains(" apply")
    {
        return true;
    }

    match family {
        "rm" => lower.contains("-rf") || lower.contains("-fr"),
        "chmod" => lower.contains(" 777"),
        "chown" => true,
        "aws" => lower.contains(" s3 rm") || lower.contains(" delete"),
        "kubectl" => lower.contains(" apply") || lower.contains(" delete"),
        "terraform" => lower.contains(" apply") || lower.contains(" destroy"),
        "helm" => lower.contains(" upgrade") || lower.contains(" uninstall"),
        "docker" => lower.contains(" push"),
        "gh" => lower.contains(" release"),
        "curl" => lower.contains("| sh") || lower.contains("| bash"),
        "ssh" | "scp" => lower.contains("prod") || lower.contains("production"),
        _ => false,
    }
}

fn correlate_sessions(
    db_path: &Path,
    repo_root: &Path,
    changed: &ChangedFiles,
) -> Result<SessionCorrelation> {
    if !db_path.exists() {
        return Ok(SessionCorrelation::default());
    }

    let conn = open_existing_database(db_path)?;
    let sessions = load_stored_sessions(&conn)?;
    let changed_paths: BTreeSet<&str> = changed
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let changed_names: BTreeSet<String> = changed
        .files
        .iter()
        .filter_map(|file| Path::new(&file.path).file_name())
        .map(|name| name.to_string_lossy().to_string())
        .collect();
    let mut correlation = SessionCorrelation::default();

    for session in sessions {
        let mut signals = Vec::new();
        if session
            .workspace_path
            .as_deref()
            .is_some_and(|path| same_path(path, repo_root))
        {
            signals.push("workspace".to_string());
        }
        if session
            .command_cwds
            .iter()
            .any(|cwd| path_inside_repo(cwd, repo_root))
        {
            signals.push("command-cwd".to_string());
        }
        if session
            .file_paths
            .iter()
            .any(|path| changed_paths.contains(path.as_str()))
        {
            signals.push("file-change".to_string());
        }

        if !signals.is_empty() {
            correlation.relevant.push(CorrelatedSession {
                id: session.id,
                codex_session_id: session.codex_session_id,
                title: session.title,
                signals,
            });
            continue;
        }

        let ambiguous = session.file_paths.iter().any(|path| {
            Path::new(path)
                .file_name()
                .is_some_and(|name| changed_names.contains(&name.to_string_lossy().to_string()))
        });
        if ambiguous {
            correlation.ambiguous.push(CorrelatedSession {
                id: session.id,
                codex_session_id: session.codex_session_id,
                title: session.title,
                signals: vec!["ambiguous-file-name".to_string()],
            });
        }
    }

    correlation
        .relevant
        .sort_by(|left, right| left.codex_session_id.cmp(&right.codex_session_id));
    correlation
        .ambiguous
        .sort_by(|left, right| left.codex_session_id.cmp(&right.codex_session_id));
    Ok(correlation)
}

fn load_stored_sessions(conn: &Connection) -> Result<Vec<StoredSession>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, codex_session_id, title, workspace_path
             FROM sessions
             ORDER BY codex_session_id",
        )
        .context("failed to load stored sessions")?;
    let mut sessions = stmt
        .query_map([], |row| {
            Ok(StoredSession {
                id: row.get(0)?,
                codex_session_id: row
                    .get::<_, Option<String>>(1)?
                    .unwrap_or_else(|| "(unknown-session)".to_string()),
                title: row.get(2)?,
                workspace_path: row.get(3)?,
                command_cwds: Vec::new(),
                file_paths: Vec::new(),
            })
        })
        .context("failed to load stored sessions")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("failed to load stored sessions")?;

    for session in &mut sessions {
        session.command_cwds = load_session_strings(conn, "commands", "cwd", session.id)?;
        session.file_paths = load_session_strings(conn, "file_changes", "path", session.id)?;
    }

    Ok(sessions)
}

fn load_session_strings(
    conn: &Connection,
    table: &str,
    column: &str,
    session_id: i64,
) -> Result<Vec<String>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {column} FROM {table}
             WHERE session_id = ?1 AND {column} IS NOT NULL
             ORDER BY {column}"
        ))
        .with_context(|| format!("failed to load {table}.{column} values"))?;
    let values = stmt
        .query_map([session_id], |row| row.get(0))
        .with_context(|| format!("failed to load {table}.{column} values"))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to load {table}.{column} values"))?;
    Ok(values)
}

fn same_path(path: &str, repo_root: &Path) -> bool {
    normalize_path(path) == normalize_path(repo_root)
}

fn path_inside_repo(path: &str, repo_root: &Path) -> bool {
    let path = normalize_path(path);
    let repo_root = normalize_path(repo_root);
    path == repo_root || path.starts_with(format!("{repo_root}/").as_str())
}

fn normalize_path(path: impl AsRef<Path>) -> String {
    path.as_ref()
        .canonicalize()
        .unwrap_or_else(|_| path.as_ref().to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn run_git<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = ProcessCommand::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_string())
}

fn run_git_in<const N: usize>(
    repo_path: &Path,
    args: [&str; N],
) -> std::result::Result<String, String> {
    let output = ProcessCommand::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn short_hash(hash: &str) -> String {
    hash.chars().take(7).collect()
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
    apply_migrations(conn, db_path)?;
    Ok(())
}

fn apply_migrations(conn: &Connection, db_path: &Path) -> Result<()> {
    let migration_version = conn
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .map_err(|source| StorageError::ApplySchema {
            path: db_path.to_path_buf(),
            source,
        })
        .with_context(|| format!("failed to initialize database {}", db_path.display()))?;

    if migration_version < 2 {
        conn.execute_batch(SCHEMA_V2)
            .map_err(|source| StorageError::ApplySchema {
                path: db_path.to_path_buf(),
                source,
            })
            .with_context(|| format!("failed to initialize database {}", db_path.display()))?;
    }

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
    for table in ["raw_events_fts", "messages_fts", "command_output_fts"] {
        check_required_fts_table(conn, db_path, table)?;
    }

    Ok(StorageStatus {
        db_path: db_path.to_path_buf(),
        migration_version,
        journal_mode,
    })
}

fn check_required_fts_table(conn: &Connection, db_path: &Path, table: &str) -> Result<()> {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |_| Ok(()))
        .map_err(|source| StorageError::InspectDb {
            path: db_path.to_path_buf(),
            source,
        })
        .with_context(|| format!("failed to inspect database {}", db_path.display()))?;
    Ok(())
}

const SCHEMA_V1: &str = r#"
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

const SCHEMA_V2: &str = r#"
DROP TABLE IF EXISTS raw_events_fts;

CREATE TABLE IF NOT EXISTS raw_events_v2 (
    id INTEGER PRIMARY KEY,
    codex_file_id INTEGER NOT NULL REFERENCES codex_files(id) ON DELETE CASCADE,
    line_number INTEGER NOT NULL,
    raw_json TEXT NOT NULL,
    line_sha256 TEXT NOT NULL,
    event_type TEXT,
    event_time TEXT,
    session_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL,
    parse_error TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (codex_file_id, line_number)
);

INSERT OR IGNORE INTO raw_events_v2 (
    id,
    codex_file_id,
    line_number,
    raw_json,
    line_sha256,
    event_type,
    event_time,
    session_id,
    parse_error,
    created_at
)
SELECT
    id,
    codex_file_id,
    line_number,
    raw_json,
    line_sha256,
    event_type,
    event_time,
    session_id,
    parse_error,
    created_at
FROM raw_events;

DROP TABLE raw_events;
ALTER TABLE raw_events_v2 RENAME TO raw_events;

CREATE VIRTUAL TABLE IF NOT EXISTS raw_events_fts
USING fts5(raw_json, parse_error, content='raw_events', content_rowid='id');

INSERT OR IGNORE INTO schema_migrations (version) VALUES (2);
PRAGMA user_version = 2;
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
        write_owner_only_file(path, text.as_bytes())
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

#[cfg(unix)]
fn write_owner_only_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_owner_only_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    fs::write(path, bytes)
}

#[cfg(unix)]
fn enforce_owner_only_permissions(path: &Path) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|source| PermissionError::SetPermissions {
            path: path.to_path_buf(),
            source,
        })
        .context("failed to set owner-only permissions")
}

#[cfg(not(unix))]
fn enforce_owner_only_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn permission_warnings(config_file: &Path, db_path: &Path) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    collect_permission_warning("config", config_file, &mut warnings)?;
    collect_permission_warning("database", db_path, &mut warnings)?;
    Ok(warnings)
}

#[cfg(not(unix))]
fn permission_warnings(_config_file: &Path, _db_path: &Path) -> Result<Vec<String>> {
    Ok(Vec::new())
}

#[cfg(unix)]
fn collect_permission_warning(label: &str, path: &Path, warnings: &mut Vec<String>) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let mode = fs::metadata(path)
        .map_err(|source| PermissionError::ReadPermissions {
            path: path.to_path_buf(),
            source,
        })
        .context("failed to inspect permissions")?
        .permissions()
        .mode()
        & 0o777;

    if mode & 0o077 != 0 {
        warnings.push(format!(
            "{label} permissions are {mode:04o}; run `chmod 600 {}` to make it owner-only",
            path.display()
        ));
    }
    Ok(())
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

#[derive(Debug, thiserror::Error)]
enum PermissionError {
    #[error("could not set owner-only permissions for {path}")]
    SetPermissions {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not read permissions for {path}")]
    ReadPermissions {
        path: PathBuf,
        source: std::io::Error,
    },
}

#[derive(Debug, thiserror::Error)]
enum IngestError {
    #[error("Codex root does not exist: {path}")]
    MissingCodexRoot { path: PathBuf },
    #[error("Codex root is not a directory: {path}")]
    InvalidCodexRoot { path: PathBuf },
}

#[derive(Debug, thiserror::Error)]
enum GitError {
    #[error("not a git repository: {path}")]
    NotRepository { path: PathBuf },
    #[error("git repository has no HEAD: {path}")]
    MissingHead { path: PathBuf },
    #[error("invalid git base ref: {reference}")]
    InvalidBaseRef { reference: String },
    #[error("could not inspect git status: {path}")]
    StatusFailed { path: PathBuf },
    #[error("could not inspect git diff: {path}")]
    DiffFailed { path: PathBuf },
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
