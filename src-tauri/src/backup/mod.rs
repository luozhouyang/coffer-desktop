//! Encrypted whole-vault backup and restore orchestration.
//!
//! Export: re-authenticate, snapshot the SQLCipher DB, then stream
//! `manifest + snapshot` through the chunked AEAD into `<dest>.partial` and
//! rename it into place. Restore: verify and decrypt into a temp DB once,
//! keep it in a [`session`], then swap it in and write the header last.

pub mod container;
pub mod session;
pub mod snapshot;

use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use zeroize::Zeroizing;

use crate::crypto::header::VaultHeader;
use crate::crypto::keys::{self, MasterKey};
use crate::error::{AppError, ErrorCode};
use crate::models::events::BackupProgress;
use crate::models::{BackupExportResult, BackupPreview};
use crate::state::AppState;
use crate::vault::VaultDb;
use crate::vault::db::{audit, now_ms, restrict_permissions};
use crate::vault::migrations::SCHEMA_VERSION;
use crate::vault::paths::VaultPaths;

use container::{
    ARCHIVE_SALT_LEN, CHUNK_SIZE, CipherParams, ContainerHeader, ENTRY_DB, ENTRY_MANIFEST,
    Manifest, ManifestCounts, NoncePrefix, PayloadParams,
};
use session::{RestoreSession, RestoreSessions};

/// Minimum bytes between two progress events, so IPC is not flooded.
const PROGRESS_MIN_BYTES: u64 = 256 * 1024;
const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(100);
/// Bytes read from the snapshot per progress check.
const STREAM_SLICE: usize = 256 * 1024;

/// Callback for `{ phase, done, total }` progress.
pub type ProgressFn<'a> = &'a mut dyn FnMut(&str, i64, i64);

/// Emits progress at most every 256 KiB or 100 ms.
struct Throttle {
    last_at: Instant,
    last_done: u64,
}

impl Throttle {
    fn new() -> Self {
        Self {
            // Dated in the past so the first update always goes through.
            last_at: Instant::now() - PROGRESS_MIN_INTERVAL,
            last_done: u64::MAX,
        }
    }

    fn tick(&mut self, emit: &mut dyn FnMut(&str, i64, i64), phase: &str, done: u64, total: u64) {
        let now = Instant::now();
        let due = now.duration_since(self.last_at) >= PROGRESS_MIN_INTERVAL
            || done.saturating_sub(self.last_done) >= PROGRESS_MIN_BYTES;
        if due {
            emit(
                phase,
                done.min(i64::MAX as u64) as i64,
                total.min(i64::MAX as u64) as i64,
            );
            self.last_at = now;
            self.last_done = done;
        }
    }
}

/// Builds a progress payload.
pub fn progress_event(phase: &str, done: i64, total: i64) -> BackupProgress {
    BackupProgress {
        phase: phase.to_string(),
        done,
        total,
    }
}

fn restore_only() -> AppError {
    AppError::new(
        ErrorCode::AlreadyInitialized,
        "Restore is only available before a vault is created.",
    )
}

fn session_not_found() -> AppError {
    AppError::new(ErrorCode::NotFound, "restore session not found or expired")
}

fn random_salt() -> [u8; ARCHIVE_SALT_LEN] {
    let mut salt = [0u8; ARCHIVE_SALT_LEN];
    crate::crypto::random_bytes(&mut salt);
    salt
}

fn derive_archive_key(
    mk: &MasterKey,
    salt: &[u8; ARCHIVE_SALT_LEN],
) -> Result<Zeroizing<[u8; 32]>, AppError> {
    let backup_key = container::derive_backup_key(mk.expose())?;
    container::derive_archive_key(&backup_key, salt)
}

fn open_private_new(path: &Path) -> Result<File, AppError> {
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let file = opts.open(path).map_err(AppError::io)?;
    restrict_permissions(path);
    Ok(file)
}

/// Renames `from` onto `to`, replacing an existing target.
fn rename_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        // Windows `rename` refuses to overwrite. The save dialog already
        // confirmed the overwrite, and `from` was fully written and fsynced in
        // the same directory, so a crash between delete and rename loses only
        // the old backup while the new one stays recoverable in `.partial`.
        if to.symlink_metadata().is_ok() {
            fs::remove_file(to)?;
        }
    }
    fs::rename(from, to)
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Validated export destination and its sibling partial file.
struct Destination {
    path: PathBuf,
    partial: PathBuf,
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

fn validate_destination(dest: &Path, paths: &VaultPaths) -> Result<Destination, AppError> {
    if !dest.is_absolute() {
        return Err(AppError::validation("destination must be an absolute path"));
    }
    // `..` would let lexical prefix checks disagree with the real location.
    if dest.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(AppError::validation("destination path is not allowed"));
    }
    let ext_ok = dest
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("coffer"));
    if !ext_ok {
        return Err(AppError::validation("destination must be a .coffer file"));
    }
    let name = dest
        .file_name()
        .ok_or_else(|| AppError::validation("destination is not allowed"))?;
    let lower = name.to_string_lossy().to_lowercase();
    if lower == "vault.db" || lower == "vault.meta.json" {
        return Err(AppError::validation("destination is not allowed"));
    }
    let parent = dest
        .parent()
        .filter(|p| p.is_dir())
        .ok_or_else(|| AppError::validation("destination folder does not exist"))?;
    if is_link(dest) || is_link(parent) {
        return Err(AppError::validation(
            "destination cannot be a symbolic link",
        ));
    }
    // Canonical paths so a symlinked ancestor cannot mask a target inside the
    // vault directory.
    let real_dest = match dest.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            let parent = parent.canonicalize().map_err(AppError::io)?;
            if is_link(&parent) {
                return Err(AppError::validation(
                    "destination cannot be a symbolic link",
                ));
            }
            parent.join(name)
        }
    };
    if real_dest.is_dir() {
        return Err(AppError::validation("destination must be a file"));
    }
    if let Ok(root) = paths.root().canonicalize()
        && real_dest.starts_with(&root)
    {
        return Err(AppError::validation(
            "destination cannot be inside the vault folder",
        ));
    }
    let partial = parent.join(format!(".{}.partial", name.to_string_lossy()));
    if is_link(&partial) {
        return Err(AppError::validation(
            "destination cannot be a symbolic link",
        ));
    }
    Ok(Destination {
        path: dest.to_path_buf(),
        partial,
    })
}

/// `backup_export`: requires an unlocked vault and the current master password.
pub fn export(
    state: &AppState,
    db: &Arc<VaultDb>,
    destination: &Path,
    password: &str,
    progress: ProgressFn<'_>,
) -> Result<BackupExportResult, AppError> {
    if !state.is_unlocked() {
        return Err(AppError::locked());
    }
    let destination = validate_destination(destination, &state.paths)?;
    let root = state.paths.root().to_path_buf();

    progress("verify", 0, 0);
    let header = VaultHeader::load(&state.paths.header())?;
    let mk = keys::unlock_with_header(&header, password)?;
    let outcome = export_with_key(db, &header, &mk, &destination, &root, progress, "password");
    drop(mk);
    match outcome {
        Ok(result) => Ok(result),
        Err(e) => {
            let _ = fs::remove_file(&destination.partial);
            Err(e)
        }
    }
}

const KEYCHAIN_KEY_MISMATCH: &str = "the saved Touch ID key does not open this vault; use your master password and enable Touch ID again";

/// Loads the keychain-cached master key. The OS prompts for Touch ID here.
fn unlock_with_keychain(state: &AppState) -> Result<MasterKey, AppError> {
    let status = state.keychain.status();
    if !status.supported || !status.available || !status.enabled {
        return Err(AppError::keychain_unavailable(
            "Touch ID export is not enabled in Settings",
        ));
    }
    let stored = state.keychain.load().map_err(|_| {
        AppError::keychain_unavailable("Touch ID was cancelled or the saved key is unavailable")
    })?;
    MasterKey::from_bytes(&stored)
        .map_err(|_| AppError::keychain_unavailable("the saved Touch ID key is invalid"))
}

/// Proves a cached master key belongs to this vault before it is used to
/// derive an archive key. A key left over from a restored or replaced vault
/// must not silently produce an unreadable backup.
fn verify_key_opens_vault(db_path: &Path, mk: &MasterKey) -> Result<(), AppError> {
    let key = keys::db_key(mk)?;
    let conn = crate::vault::db::open_encrypted(db_path, &key)
        .map_err(|_| AppError::keychain_unavailable(KEYCHAIN_KEY_MISMATCH))?;
    let opened = conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    });
    drop(conn);
    opened
        .map(|_| ())
        .map_err(|_| AppError::keychain_unavailable(KEYCHAIN_KEY_MISMATCH))
}

/// `backup_export_keychain`: same output as [`export`], authenticated by the
/// biometric keychain instead of a typed master password.
pub fn export_keychain(
    state: &AppState,
    db: &Arc<VaultDb>,
    destination: &Path,
    progress: ProgressFn<'_>,
) -> Result<BackupExportResult, AppError> {
    if !state.is_unlocked() {
        return Err(AppError::locked());
    }
    let destination = validate_destination(destination, &state.paths)?;
    let root = state.paths.root().to_path_buf();

    progress("verify", 0, 0);
    let header = VaultHeader::load(&state.paths.header())?;
    let mk = unlock_with_keychain(state)?;
    let outcome = verify_key_opens_vault(&state.paths.db(), &mk).and_then(|_| {
        export_with_key(db, &header, &mk, &destination, &root, progress, "biometric")
    });
    drop(mk);
    match outcome {
        Ok(result) => Ok(result),
        Err(e) => {
            let _ = fs::remove_file(&destination.partial);
            Err(e)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn export_with_key(
    db: &Arc<VaultDb>,
    vault_header: &VaultHeader,
    mk: &MasterKey,
    destination: &Destination,
    root: &Path,
    progress: ProgressFn<'_>,
    auth: &str,
) -> Result<BackupExportResult, AppError> {
    progress("snapshot", 0, 0);
    let created_at_ms = now_ms();
    let snap = snapshot::create(root, db, &crate::crypto::keys::db_key(mk)?)?;
    let counts = snap.counts;
    let snapshot_path = snap.take();
    let result = write_and_publish(
        &snapshot_path,
        counts,
        vault_header,
        mk,
        destination,
        created_at_ms,
        progress,
    );
    snapshot::remove_db_files(&snapshot_path);
    let (bytes, counts) = result?;
    let meta = json!({
        "items": counts.items,
        "projects": counts.projects,
        "bytes": bytes,
        "auth": auth,
    });
    db.with(|conn| audit(conn, "exportBackup", None, Some(&meta.to_string())))?;
    Ok(BackupExportResult {
        path: destination.path.to_string_lossy().into_owned(),
        bytes: i64::try_from(bytes).unwrap_or(i64::MAX),
        item_count: counts.items,
        project_count: counts.projects,
        created_at_ms,
    })
}

#[allow(clippy::too_many_arguments)]
fn write_and_publish(
    snapshot_path: &Path,
    counts: snapshot::SnapshotCounts,
    vault_header: &VaultHeader,
    mk: &MasterKey,
    destination: &Destination,
    created_at_ms: i64,
    progress: ProgressFn<'_>,
) -> Result<(u64, ManifestCounts), AppError> {
    let partial = destination.partial.as_path();
    let db_len = fs::metadata(snapshot_path).map_err(AppError::io)?.len();
    let manifest = Manifest {
        schema_version: counts.schema_version,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        source_vault_id: vault_header.vault_id.clone(),
        created_at_ms,
        counts: ManifestCounts {
            items: counts.items,
            projects: counts.projects,
            tags: counts.tags,
        },
    };
    let manifest_bytes =
        Zeroizing::new(serde_json::to_vec(&manifest).map_err(|_| AppError::internal("serialize"))?);
    let plain_size = container::framed_plain_size(manifest_bytes.len(), db_len)
        .ok_or_else(|| AppError::internal("backup is too large"))?;
    let salt = random_salt();
    let prefix = NoncePrefix::random();
    let header = ContainerHeader {
        format_version: container::FORMAT_VERSION,
        kind: container::KIND_BACKUP.to_string(),
        created_at_ms,
        source_vault_id: vault_header.vault_id.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        archive_salt_hex: hex::encode(salt),
        cipher: CipherParams {
            alg: container::ALG_XCHACHA_CHUNKED.to_string(),
            nonce_prefix_hex: prefix.hex(),
            chunk_size: CHUNK_SIZE as u32,
        },
        payload: PayloadParams {
            codec: container::CODEC_VAULT_SNAPSHOT.to_string(),
            plain_size,
        },
        vault_header: vault_header.clone(),
    };
    let header_bytes = container::encode_header(&header)?;
    let archive_key = derive_archive_key(mk, &salt)?;

    progress("encrypt", 0, plain_size.min(i64::MAX as u64) as i64);
    let mut file = open_private_new(partial)?;
    container::write_header(&mut file, &header_bytes)?;
    let mut enc = container::ChunkEncryptor::new(file, &archive_key, prefix, &header_bytes);
    let mut throttle = Throttle::new();
    // Advance progress only in `encrypt` once the manifest is framed; the
    // initial 0-of-plainSize update already told the UI the phase began.
    container::write_entry_header(&mut enc, ENTRY_MANIFEST, manifest_bytes.len() as u64)?;
    enc.write_all(&manifest_bytes)?;
    container::write_entry_header(&mut enc, ENTRY_DB, db_len)?;
    let mut source = BufReader::new(File::open(snapshot_path).map_err(AppError::io)?);
    let mut remaining = db_len;
    let mut scratch = Zeroizing::new(vec![0u8; STREAM_SLICE]);
    while remaining > 0 {
        let want = remaining.min(scratch.len() as u64) as usize;
        let n = source.read(&mut scratch[..want]).map_err(AppError::io)?;
        if n == 0 {
            return Err(AppError::internal(
                "database snapshot changed while exporting",
            ));
        }
        enc.write_all(&scratch[..n])?;
        remaining -= n as u64;
        // The entry header counts toward plainSize too.
        throttle.tick(progress, "encrypt", enc.written(), plain_size);
    }
    if enc.written() != plain_size {
        return Err(AppError::internal("backup payload size mismatch"));
    }
    let file = enc.finish()?;
    file.sync_all().map_err(AppError::io)?;
    drop(file);
    let bytes = fs::metadata(partial).map_err(AppError::io)?.len();
    rename_replace(partial, &destination.path).map_err(AppError::io)?;
    progress(
        "done",
        bytes.min(i64::MAX as u64) as i64,
        bytes.min(i64::MAX as u64) as i64,
    );
    Ok((bytes, manifest.counts))
}

// ---------------------------------------------------------------------------
// Restore
// ---------------------------------------------------------------------------

/// Checks the user-chosen archive before it is copied into the vault root.
fn validate_source(path: &Path) -> Result<(), AppError> {
    let meta = fs::symlink_metadata(path)
        .map_err(|_| AppError::validation("backup file could not be opened"))?;
    if meta.file_type().is_symlink() {
        return Err(AppError::validation(
            "backup file cannot be a symbolic link",
        ));
    }
    if !meta.is_file() {
        return Err(AppError::validation("backup file is not a regular file"));
    }
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("coffer"))
    {
        return Err(AppError::unsupported_format(
            "backup file must have a .coffer extension",
        ));
    }
    if meta.len() > container::MAX_FILE_SIZE {
        return Err(AppError::unsupported_format("backup file is too large"));
    }
    Ok(())
}

/// `backup_restore_begin`: decrypts and verifies the archive once, leaving a
/// verified encrypted temp DB in a new session.
pub fn restore_begin(
    state: &AppState,
    password: &str,
    source: &Path,
    progress: ProgressFn<'_>,
) -> Result<BackupPreview, AppError> {
    if state.paths.initialized() {
        return Err(restore_only());
    }
    validate_source(source)?;
    state.paths.ensure_dir()?;
    let root = state.paths.root().to_path_buf();
    let temp_db = session::temp_db_path(&state.paths);
    snapshot::remove_db_files(&temp_db);

    let staged = session::stage_archive(&root, source)?;
    let outcome = restore_from_archive(&staged, password, &temp_db, progress);
    let _ = fs::remove_file(&staged);
    match outcome {
        Ok(session) => {
            let preview = session.preview.clone();
            state
                .restore_sessions
                .lock()
                .map_err(|_| AppError::internal("state busy"))?
                .insert(session);
            Ok(preview)
        }
        Err(e) => {
            // A failed or abandoned preview must never leave a plaintext or
            // partially written candidate behind.
            snapshot::remove_db_files(&temp_db);
            Err(e)
        }
    }
}

fn restore_from_archive(
    staged: &Path,
    password: &str,
    temp_db: &Path,
    progress: ProgressFn<'_>,
) -> Result<RestoreSession, AppError> {
    let mut file = BufReader::new(File::open(staged).map_err(AppError::io)?);
    let (header, header_bytes) = container::read_header(&mut file)?;

    progress("verify", 0, 0);
    let mk = keys::unlock_with_header(&header.vault_header, password)?;
    let db_key = crate::crypto::keys::db_key(&mk)?;
    let archive_key = derive_archive_key(&mk, &*header.archive_salt()?)?;
    drop(mk);

    progress(
        "decrypt",
        0,
        header.payload.plain_size.min(i64::MAX as u64) as i64,
    );
    let mut manifest: Option<Manifest> = None;
    let mut temp_out: Option<File> = None;
    let mut throttle = Throttle::new();
    let decrypt_result = container::decrypt(
        file,
        &header,
        &header_bytes,
        &archive_key,
        |done, total| throttle.tick(progress, "decrypt", done, total),
        |kind, entry| {
            if kind == ENTRY_MANIFEST {
                let bytes = entry.read_all()?;
                let parsed: Manifest = serde_json::from_slice(&bytes)
                    .map_err(|_| container::unsupported("backup manifest is malformed"))?;
                manifest = Some(parsed);
            } else {
                let mut out = open_private_new(temp_db)?;
                entry.copy_to(&mut out)?;
                out.sync_all().map_err(AppError::io)?;
                temp_out = Some(out);
            }
            Ok(())
        },
    );
    drop(temp_out);
    decrypt_result?;

    progress("verify", 0, 0);
    let manifest = manifest.ok_or_else(|| container::unsupported("backup manifest is missing"))?;
    if manifest.schema_version > SCHEMA_VERSION {
        return Err(container::unsupported(
            "backup was created by a newer version of Coffer",
        ));
    }
    if manifest.source_vault_id != header.vault_header.vault_id {
        return Err(container::unsupported("backup manifest is inconsistent"));
    }
    let version = snapshot::user_version(temp_db, &db_key)?;
    if version > SCHEMA_VERSION {
        return Err(container::unsupported(
            "backup was created by a newer version of Coffer",
        ));
    }
    snapshot::quick_check(temp_db, &db_key)?;
    let counts = snapshot::inspect(temp_db, &db_key)?;

    let preview = BackupPreview {
        session_id: String::new(),
        source_vault_id: manifest.source_vault_id,
        created_at_ms: manifest.created_at_ms,
        app_version: manifest.app_version,
        schema_version: manifest.schema_version,
        item_count: counts.items,
        project_count: counts.projects,
        tag_count: counts.tags,
    };
    progress("done", 0, 0);
    Ok(RestoreSession::new(
        temp_db.to_path_buf(),
        header.vault_header,
        db_key,
        preview,
    ))
}

/// `backup_restore_commit`: swaps the verified temp DB in, writes the header
/// last, then opens and unlocks the vault.
pub fn restore_commit(
    state: &AppState,
    sessions: &mut RestoreSessions,
    session_id: &str,
) -> Result<VaultDb, AppError> {
    if state.paths.initialized() {
        return Err(restore_only());
    }
    let session = sessions.remove(session_id).ok_or_else(session_not_found)?;
    let paths = state.paths.clone();
    paths.ensure_dir()?;
    let temp = session.temp_db_path().to_path_buf();
    snapshot::quick_check(&temp, &session.db_key)?;

    let db_path = paths.db();
    if db_path.exists() {
        // Leftover from a crashed restore: keep it for inspection, never
        // delete user data.
        let stamp = now_ms();
        let orphan = paths.root().join(format!("vault.db.orphan-{stamp}"));
        fs::rename(&db_path, &orphan).map_err(AppError::io)?;
        for suffix in ["wal", "shm"] {
            let from = sidecar(&db_path, suffix);
            if from.exists() {
                fs::rename(&from, sidecar(&orphan, suffix)).map_err(AppError::io)?;
            }
        }
    }
    fs::rename(&temp, &db_path).map_err(AppError::io)?;
    restrict_permissions(&db_path);
    // The header is the initialization marker: it must land only after the DB
    // is fully in place.
    session.vault_header.store(&paths.header())?;
    restrict_permissions(&paths.header());

    let db = VaultDb::open(&db_path, &session.db_key)?;
    let meta = json!({
        "sourceVaultId": session.preview.source_vault_id,
        "items": session.preview.item_count,
        "projects": session.preview.project_count,
    });
    let meta = meta.to_string();
    drop(session);
    db.with(|conn| audit(conn, "restore", None, Some(&meta)))?;
    Ok(db)
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!("-{suffix}"));
    PathBuf::from(name)
}

/// `backup_restore_cancel`: drops the session and its temp DB.
pub fn restore_cancel(sessions: &mut RestoreSessions, session_id: &str) -> Result<(), AppError> {
    sessions.remove(session_id);
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::crypto::KdfParams;
    use crate::models::ItemInput;
    use crate::vault::ops;
    use uuid::Uuid;

    pub(crate) const PASSWORD: &str = "correct horse battery";
    const TITLE_MARKER: &str = "plaintext-title-marker-xyz";
    const SECRET_MARKER: &str = "secret-value-marker-xyz";

    pub(crate) fn sample_header() -> VaultHeader {
        VaultHeader {
            version: 1,
            vault_id: Uuid::now_v7().to_string(),
            kdf: "argon2id".into(),
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
            salt_hex: hex::encode([1u8; 32]),
            wrap_nonce_hex: hex::encode([2u8; 24]),
            wrapped_mk_hex: hex::encode([3u8; 48]),
        }
    }

    fn test_state(dir: &tempfile::TempDir) -> AppState {
        let clipboard = Arc::new(crate::clipboard::ClipboardManager::new(Arc::new(
            crate::clipboard::tests::FakeClipboard::default(),
        )));
        let keychain = Arc::new(crate::keychain::tests::FakeKeychain::new());
        AppState::new(VaultPaths::from_app_data(dir.path()), clipboard, keychain)
    }

    /// Creates a vault with a project, two items and tags; returns the state.
    fn source_vault(dir: &tempfile::TempDir) -> AppState {
        let state = test_state(dir);
        state.paths.ensure_dir().unwrap();
        // Fast KDF params keep Argon2 from dominating the test run.
        let created = keys::create(
            PASSWORD,
            KdfParams {
                m_cost: 8,
                t_cost: 1,
                p_cost: 1,
            },
        )
        .unwrap();
        created.header.store(&state.paths.header()).unwrap();
        let key = keys::db_key(&created.mk).unwrap();
        let db = VaultDb::create(&state.paths.db(), &key).unwrap();
        let project = ops::create_project(
            &db,
            &crate::models::ProjectInput {
                name: "Prod".into(),
                description: None,
                color: None,
            },
        )
        .unwrap();
        ops::create_item(
            &db,
            &ItemInput {
                kind: crate::models::ItemKind::ApiKey,
                title: TITLE_MARKER.into(),
                secret: SECRET_MARKER.into(),
                username: Some("svc".into()),
                url: None,
                project_id: Some(project.id.clone()),
                notes: Some("notes".into()),
                tags: vec!["alpha".into(), "beta".into()],
            },
        )
        .unwrap();
        ops::create_item(
            &db,
            &ItemInput {
                kind: crate::models::ItemKind::Note,
                title: "Second item".into(),
                secret: "another value".into(),
                username: None,
                url: None,
                project_id: None,
                notes: None,
                tags: vec!["alpha".into()],
            },
        )
        .unwrap();
        state.set_unlocked(db).unwrap();
        state
    }

    fn store_mk_in_keychain(state: &AppState) {
        let header = VaultHeader::load(&state.paths.header()).unwrap();
        let mk = keys::unlock_with_header(&header, PASSWORD).unwrap();
        state.keychain.store(mk.expose()).unwrap();
    }

    fn no_progress() -> impl FnMut(&str, i64, i64) {
        |_, _, _| {}
    }

    fn export_to(
        state: &AppState,
        dest: &Path,
        password: &str,
    ) -> Result<BackupExportResult, AppError> {
        let db = state.require_db().unwrap();
        let mut progress = no_progress();
        export(state, &db, dest, password, &mut progress)
    }

    fn restore_into(
        state: &AppState,
        archive: &Path,
        password: &str,
    ) -> Result<BackupPreview, AppError> {
        let mut progress = no_progress();
        restore_begin(state, password, archive, &mut progress)
    }

    fn commit(state: &AppState, session_id: &str) -> Result<VaultDb, AppError> {
        let mut sessions = state.restore_sessions.lock().unwrap();
        restore_commit(state, &mut sessions, session_id)
    }

    fn snapshot_titles(db: &VaultDb) -> Vec<String> {
        db.with(|c| {
            let mut st =
                c.prepare("SELECT title FROM items WHERE deleted_at IS NULL ORDER BY title")?;
            Ok(st
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap()
    }

    fn item_id(db: &VaultDb, title: &str) -> String {
        db.with(|c| {
            Ok(
                c.query_row("SELECT id FROM items WHERE title = ?1", [title], |r| {
                    r.get::<_, String>(0)
                })?,
            )
        })
        .unwrap()
    }

    #[test]
    fn export_then_restore_round_trips_everything() {
        let src_dir = tempfile::tempdir().unwrap();
        let dst_dir = tempfile::tempdir().unwrap();
        let out_dir = tempfile::tempdir().unwrap();
        let source = source_vault(&src_dir);
        let db = source.require_db().unwrap();
        let titles_before = snapshot_titles(&db);
        let projects_before: Vec<String> = ops::list_projects(&db, false)
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();

        let archive = out_dir.path().join("backup.coffer");
        let result = export_to(&source, &archive, PASSWORD).unwrap();
        assert_eq!(result.path, archive.to_string_lossy());
        assert_eq!(result.item_count, 2);
        assert_eq!(result.project_count, 1);
        assert!(result.bytes > 0);
        assert!(archive.is_file());

        // Nothing secret survives on disk in the archive or next to it.
        let raw = fs::read(&archive).unwrap();
        for marker in [TITLE_MARKER, SECRET_MARKER, "Second item"] {
            assert!(
                !raw.windows(marker.len()).any(|w| w == marker.as_bytes()),
                "archive leaks {marker}"
            );
        }
        assert_eq!(
            fs::read_dir(out_dir.path()).unwrap().flatten().count(),
            1,
            "no .partial left behind"
        );

        let exports: i64 = db
            .with(|c| {
                Ok(c.query_row(
                    "SELECT count(*) FROM audit_log WHERE action = 'exportBackup'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(exports, 1);

        // Restore into a second, uninitialized vault.
        let target = test_state(&dst_dir);
        let preview = restore_into(&target, &archive, PASSWORD).unwrap();
        assert_eq!(preview.item_count, 2);
        assert_eq!(preview.project_count, 1);
        // Auto-classification adds tags of its own; only the user tags matter.
        assert!(preview.tag_count >= 2);
        assert_eq!(preview.schema_version, SCHEMA_VERSION);
        let source_vault_id = crate::crypto::header::VaultHeader::load(&source.paths.header())
            .unwrap()
            .vault_id;
        assert_eq!(preview.source_vault_id, source_vault_id);
        assert!(!target.paths.initialized(), "begin must not initialize");
        let left: Vec<String> = fs::read_dir(target.paths.root())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!left.iter().any(|n| n.starts_with("restore-")), "{left:?}");

        // Cancel drops the session and its temp DB; begin again, then commit.
        let first = preview.session_id.clone();
        {
            let mut sessions = target.restore_sessions.lock().unwrap();
            restore_cancel(&mut sessions, &first).unwrap();
        }
        assert!(!session::temp_db_path(&target.paths).exists());
        let preview = restore_into(&target, &archive, PASSWORD).unwrap();
        let restored = commit(&target, &preview.session_id).unwrap();
        target.set_unlocked(restored).unwrap();
        assert!(target.paths.initialized());

        let db = target.require_db().unwrap();
        assert_eq!(snapshot_titles(&db), titles_before);
        assert_eq!(
            ops::list_projects(&db, false)
                .unwrap()
                .into_iter()
                .map(|p| p.name)
                .collect::<Vec<_>>(),
            projects_before
        );
        let id = item_id(&db, TITLE_MARKER);
        let tags = db.with(|c| crate::vault::db::item_tags(c, &id)).unwrap();
        assert_eq!(tags, vec!["alpha".to_string(), "beta".to_string()]);
        let secret = db.with(|c| crate::vault::ops::read_secret(c, &id)).unwrap();
        assert_eq!(secret.as_str(), SECRET_MARKER);
        let restores: i64 = db
            .with(|c| {
                Ok(c.query_row(
                    "SELECT count(*) FROM audit_log WHERE action = 'restore'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(restores, 1);
        assert!(
            target
                .restore_sessions
                .lock()
                .unwrap()
                .get(&preview.session_id)
                .is_none()
        );
        // The restored WAL sidecars and no leftover temp DB.
        assert!(!session::temp_db_path(&target.paths).exists());
    }

    #[test]
    fn keychain_export_requires_a_key_that_opens_this_vault() {
        let src_dir = tempfile::tempdir().unwrap();
        let out_dir = tempfile::tempdir().unwrap();
        let source = source_vault(&src_dir);
        let db = source.require_db().unwrap();
        let archive = out_dir.path().join("biometric.coffer");
        let mut progress = no_progress();

        // Nothing stored yet: no biometric credential.
        assert!(matches!(
            export_keychain(&source, &db, &archive, &mut progress),
            Err(e) if e.code == ErrorCode::KeychainUnavailable
        ));

        // A stale 32-byte key must not produce an unreadable backup.
        source.keychain.store(&[9u8; 32]).unwrap();
        assert!(matches!(
            export_keychain(&source, &db, &archive, &mut progress),
            Err(e) if e.code == ErrorCode::KeychainUnavailable
        ));
        assert!(!archive.exists());

        store_mk_in_keychain(&source);
        let result = export_keychain(&source, &db, &archive, &mut progress).unwrap();
        assert!(archive.is_file());
        assert_eq!(result.item_count, 2);

        let meta: String = db
            .with(|c| {
                Ok(c.query_row(
                    "SELECT meta FROM audit_log WHERE action = 'exportBackup' \
                     ORDER BY id DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert!(meta.contains("\"auth\":\"biometric\""));
    }

    #[test]
    fn wrong_password_and_malformed_archives_are_rejected() {
        let src_dir = tempfile::tempdir().unwrap();
        let dst_dir = tempfile::tempdir().unwrap();
        let out_dir = tempfile::tempdir().unwrap();
        let source = source_vault(&src_dir);
        let archive = out_dir.path().join("backup.coffer");
        assert!(matches!(
            export_to(&source, &archive, "wrong password"),
            Err(e) if e.code == ErrorCode::InvalidPassword
        ));
        assert!(!archive.exists(), "failed export leaves no output");
        export_to(&source, &archive, PASSWORD).unwrap();

        let target = test_state(&dst_dir);
        assert!(matches!(
            restore_into(&target, &archive, "wrong password"),
            Err(e) if e.code == ErrorCode::InvalidPassword
        ));
        assert!(!target.paths.initialized());

        let bad = out_dir.path().join("bad.coffer");
        fs::write(&bad, b"not a coffer file").unwrap();
        assert!(matches!(
            restore_into(&target, &bad, PASSWORD),
            Err(e) if e.code == ErrorCode::UnsupportedFormat
        ));

        // Flip plaintext bits: either the AEAD rejects the chunk or the
        // decrypted DB fails its key/integrity checks; never a silent success.
        let mut corrupt = fs::read(&archive).unwrap();
        let at = corrupt.len() / 2;
        corrupt[at] ^= 0xff;
        fs::write(&bad, &corrupt).unwrap();
        assert!(matches!(
            restore_into(&target, &bad, PASSWORD),
            Err(ref e)
                if e.code == ErrorCode::UnsupportedFormat || e.code == ErrorCode::Io
        ));

        let wrong_ext = out_dir.path().join("backup.zip");
        fs::write(&wrong_ext, b"x").unwrap();
        assert!(matches!(
            restore_into(&target, &wrong_ext, PASSWORD),
            Err(e) if e.code == ErrorCode::UnsupportedFormat
        ));
        // Nothing was initialized and no temp DB was left behind.
        assert!(!target.paths.db().exists());
        assert!(!session::temp_db_path(&target.paths).exists());
    }

    #[test]
    fn restore_into_initialized_vault_is_rejected() {
        let src_dir = tempfile::tempdir().unwrap();
        let dst_dir = tempfile::tempdir().unwrap();
        let out_dir = tempfile::tempdir().unwrap();
        let source = source_vault(&src_dir);
        let archive = out_dir.path().join("backup.coffer");
        export_to(&source, &archive, PASSWORD).unwrap();

        let target = test_state(&dst_dir);
        source_vault(&dst_dir);
        let err = restore_into(&target, &archive, PASSWORD).unwrap_err();
        assert_eq!(err.code, ErrorCode::AlreadyInitialized);
        assert_eq!(
            err.message,
            "Restore is only available before a vault is created."
        );
    }

    #[test]
    fn destination_validation() {
        let src_dir = tempfile::tempdir().unwrap();
        let out_dir = tempfile::tempdir().unwrap();
        let source = source_vault(&src_dir);
        let ok = out_dir.path().join("ok.coffer");
        assert!(export_to(&source, &ok, PASSWORD).is_ok());

        for bad in [
            out_dir.path().join("backup.zip"),
            out_dir.path().join("vault.db"),
            source.paths.root().join("inside.coffer"),
            out_dir.path().join("missing-dir").join("x.coffer"),
        ] {
            assert!(
                matches!(
                    export_to(&source, &bad, PASSWORD),
                    Err(e) if e.code == ErrorCode::Validation
                ),
                "{bad:?}"
            );
        }
        // Case-insensitive extension is accepted.
        let upper = out_dir.path().join("UPPER.COFFER");
        assert!(export_to(&source, &upper, PASSWORD).is_ok());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&upper).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn export_requires_unlock_and_restore_commit_requires_session() {
        let src_dir = tempfile::tempdir().unwrap();
        let out_dir = tempfile::tempdir().unwrap();
        let state = test_state(&src_dir);
        let dest = out_dir.path().join("x.coffer");
        let db = VaultDb::create(
            &src_dir.path().join("scratch.db"),
            &Zeroizing::new([1u8; 32]),
        )
        .unwrap();
        let mut progress = no_progress();
        assert!(matches!(
            export(&state, &Arc::new(db), &dest, PASSWORD, &mut progress),
            Err(e) if e.code == ErrorCode::Locked
        ));

        let target = test_state(&tempfile::tempdir().unwrap());
        assert!(matches!(
            commit(&target, "missing"),
            Err(e) if e.code == ErrorCode::NotFound
        ));
        let mut sessions = target.restore_sessions.lock().unwrap();
        assert!(restore_cancel(&mut sessions, "missing").is_ok());
    }

    #[test]
    fn startup_cleanup_removes_stale_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_state(&dir);
        state.paths.ensure_dir().unwrap();
        for name in [
            "restore-x.coffer",
            "vault.db.restore-x.tmp",
            "snapshot-x.db",
        ] {
            fs::write(state.paths.root().join(name), b"x").unwrap();
        }
        session::cleanup_on_startup(state.paths.root()).unwrap();
        for name in [
            "restore-x.coffer",
            "vault.db.restore-x.tmp",
            "snapshot-x.db",
        ] {
            assert!(!state.paths.root().join(name).exists());
        }
    }

    #[test]
    fn commit_renames_an_orphan_db_instead_of_deleting_it() {
        let src_dir = tempfile::tempdir().unwrap();
        let dst_dir = tempfile::tempdir().unwrap();
        let out_dir = tempfile::tempdir().unwrap();
        let source = source_vault(&src_dir);
        let archive = out_dir.path().join("backup.coffer");
        export_to(&source, &archive, PASSWORD).unwrap();

        let target = test_state(&dst_dir);
        fs::create_dir_all(target.paths.root()).unwrap();
        fs::write(target.paths.db(), b"orphan").unwrap();
        let preview = restore_into(&target, &archive, PASSWORD).unwrap();
        commit(&target, &preview.session_id).unwrap();
        let orphans: Vec<_> = fs::read_dir(target.paths.root())
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("vault.db.orphan-")
            })
            .collect();
        assert_eq!(orphans.len(), 1);
        assert_eq!(fs::read(orphans[0].path()).unwrap(), b"orphan");
    }
}
