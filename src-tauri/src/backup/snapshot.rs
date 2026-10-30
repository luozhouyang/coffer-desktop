//! Consistent SQLCipher snapshots for export, plus read-only verification of
//! restored database files.

use std::fs::File;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, params};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::error::AppError;
use crate::vault::VaultDb;
use crate::vault::db::{open_encrypted, restrict_permissions};
use crate::vault::migrations;

const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";

/// Counters read from a database snapshot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SnapshotCounts {
    pub schema_version: i64,
    pub items: i64,
    pub projects: i64,
    pub tags: i64,
}

/// A SQLCipher snapshot on disk. Removes itself and its sidecars on drop.
#[derive(Debug)]
pub struct Snapshot {
    path: PathBuf,
    pub counts: SnapshotCounts,
}

impl Snapshot {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Hands the file to the caller; the handle no longer removes it.
    pub fn take(mut self) -> PathBuf {
        let path = std::mem::take(&mut self.path);
        remove_sidecars(&path);
        path
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        if !self.path.as_os_str().is_empty() {
            remove_db_files(&self.path);
        }
    }
}

pub fn remove_db_files(path: &Path) {
    let _ = std::fs::remove_file(path);
    remove_sidecars(path);
}

pub fn remove_sidecars(path: &Path) {
    let _ = std::fs::remove_file(sidecar(path, "wal"));
    let _ = std::fs::remove_file(sidecar(path, "shm"));
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!("-{suffix}"));
    PathBuf::from(name)
}

/// Creates `<root>/snapshot-<uuid>.db`, encrypted with `db_key`, holding a
/// consistent copy of the live vault.
pub fn create(
    root: &Path,
    db: &VaultDb,
    db_key: &Zeroizing<[u8; 32]>,
) -> Result<Snapshot, AppError> {
    std::fs::create_dir_all(root).map_err(AppError::io)?;
    let path = root.join(format!("snapshot-{}.db", Uuid::now_v7()));
    remove_db_files(&path);

    let vacuumed = vacuum_into(db, &path).and_then(|_| {
        restrict_permissions(&path);
        verify_encrypted(&path, db_key)
    });
    if let Err(e) = vacuumed {
        // The destination may hold a partial or plaintext result; never leave
        // it lying around before falling back.
        remove_db_files(&path);
        if e.code == crate::error::ErrorCode::Io {
            return Err(e);
        }
        backup_api(db, &path, db_key)?;
    }

    restrict_permissions(&path);
    let counts = inspect(&path, db_key)?;
    Ok(Snapshot { path, counts })
}

/// `VACUUM INTO` runs inside the live connection, so it sees a consistent
/// committed state and the SQLCipher codec writes the destination encrypted.
fn vacuum_into(db: &VaultDb, dest: &Path) -> Result<(), AppError> {
    let dest = dest.to_string_lossy().into_owned();
    db.with(|conn| {
        conn.execute("VACUUM INTO ?1", params![dest])?;
        Ok(())
    })
}

/// Fallback page-by-page copy through rusqlite's online backup API.
fn backup_api(db: &VaultDb, dest: &Path, db_key: &Zeroizing<[u8; 32]>) -> Result<(), AppError> {
    db.with(|src| {
        let mut dst = Connection::open(dest)?;
        let key_hex = hex::encode(db_key.as_ref());
        // Pages are copied verbatim, so the destination must carry the same
        // key: the codec then leaves page bytes untouched (same salt) and the
        // result decrypts with the vault's DB key.
        dst.pragma_update(None, "key", format!("x'{key_hex}'"))?;
        let backup = rusqlite::backup::Backup::new(src, &mut dst)?;
        backup.run_to_completion(100, std::time::Duration::from_millis(5), None)?;
        Ok(())
    })?;
    Ok(())
}

/// Proves the snapshot is encrypted (not a plaintext SQLite file) and opens
/// with the vault's DB key.
pub fn verify_encrypted(path: &Path, db_key: &Zeroizing<[u8; 32]>) -> Result<(), AppError> {
    use std::io::Read;
    let mut magic = [0u8; SQLITE_MAGIC.len()];
    let mut file = File::open(path).map_err(AppError::io)?;
    file.read_exact(&mut magic).map_err(AppError::io)?;
    if magic == SQLITE_MAGIC {
        // Refuse to ship a plaintext snapshot even if SQLCipher is misconfigured.
        return Err(AppError::internal("database snapshot is not encrypted"));
    }
    drop(file);
    let conn = open_encrypted(path, db_key)?;
    drop(conn);
    Ok(())
}

/// Opens a database read-only and checks its key, integrity and counters.
/// Never runs migrations.
pub fn inspect(path: &Path, db_key: &Zeroizing<[u8; 32]>) -> Result<SnapshotCounts, AppError> {
    let conn = connect_read_only(path, db_key)?;
    let quick: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if quick != "ok" {
        return Err(AppError::internal("database integrity check failed"));
    }
    let schema_version = migrations::current_version(&conn)?;
    let items = count(&conn, "items")?;
    let projects = count(&conn, "projects")?;
    let tags = count(&conn, "tags")?;
    Ok(SnapshotCounts {
        schema_version,
        items,
        projects,
        tags,
    })
}

fn count(conn: &Connection, table: &str) -> Result<i64, AppError> {
    Ok(conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?)
}

/// Opens a database without WAL side effects; used for files we did not
/// create (restore candidates) so verification cannot alter them.
pub fn connect_read_only(
    path: &Path,
    db_key: &Zeroizing<[u8; 32]>,
) -> Result<Connection, AppError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let key_hex = hex::encode(db_key.as_ref());
    conn.pragma_update(None, "key", format!("x'{key_hex}'"))?;
    // Touch the schema so a wrong key or a foreign file fails right here.
    match conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    }) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(code, _))
            if code.code == rusqlite::ErrorCode::NotADatabase =>
        {
            return Err(AppError::unsupported_format(
                "backup database could not be decrypted",
            ));
        }
        Err(e) => return Err(e.into()),
    }
    Ok(conn)
}

/// Runs `PRAGMA quick_check` on a restored database with the DB key.
pub fn quick_check(path: &Path, db_key: &Zeroizing<[u8; 32]>) -> Result<(), AppError> {
    let conn = connect_read_only(path, db_key)?;
    let quick: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if quick != "ok" {
        return Err(AppError::unsupported_format(
            "backup database failed its integrity check",
        ));
    }
    Ok(())
}

/// Reads `user_version` with the DB key.
pub fn user_version(path: &Path, db_key: &Zeroizing<[u8; 32]>) -> Result<i64, AppError> {
    let conn = connect_read_only(path, db_key)?;
    migrations::current_version(&conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded(root: &Path) -> (VaultDb, Zeroizing<[u8; 32]>) {
        let key = Zeroizing::new([11u8; 32]);
        let db = VaultDb::create(&root.join("vault.db"), &key).unwrap();
        db.with(|c| {
            c.execute_batch(
                "INSERT INTO projects VALUES('p1','P',NULL,NULL,NULL,1,1);
                 INSERT INTO items VALUES('i1','note','plaintext-title-marker',NULL,NULL,'p1',\
                 '{{SECRET_marker}}',NULL,NULL,'default',0,1,1,NULL);
                 INSERT INTO tags VALUES('t1','prod',NULL);
                 INSERT INTO item_tags VALUES('i1','t1');",
            )?;
            Ok(())
        })
        .unwrap();
        (db, key)
    }

    #[test]
    fn vacuum_into_produces_an_encrypted_consistent_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let (db, key) = seeded(dir.path());
        let snap = create(dir.path(), &db, &key).unwrap();
        let path = snap.path().to_path_buf();
        assert_eq!(
            snap.counts,
            SnapshotCounts {
                schema_version: migrations::SCHEMA_VERSION,
                items: 1,
                projects: 1,
                tags: 1,
            }
        );
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.windows(21).any(|w| w == b"plaintext-title-marker"));
        assert!(!raw.starts_with(SQLITE_MAGIC));
        verify_encrypted(&path, &key).unwrap();
        assert!(verify_encrypted(&path, &Zeroizing::new([12u8; 32])).is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        drop(snap);
        assert!(!path.exists(), "snapshot handle removes the file");
        db.close();
    }

    #[test]
    fn take_moves_the_file_out() {
        let dir = tempfile::tempdir().unwrap();
        let (db, key) = seeded(dir.path());
        let snap = create(dir.path(), &db, &key).unwrap();
        let path = snap.take();
        assert!(path.exists());
        remove_db_files(&path);
        db.close();
    }
}
