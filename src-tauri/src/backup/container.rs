//! `.coffer` container: magic, authenticated header, framed payload and the
//! chunked XChaCha20-Poly1305 envelope.
//!
//! ```text
//! magic "COFFEX01" | u32 LE header_len | header JSON | chunk 0 | chunk 1 | …
//! chunk := ciphertext(body) | tag(16B)        body <= 64 KiB
//! nonce := noncePrefix (20B) || u32 BE counter
//! AAD   := SHA-256(header JSON bytes)
//! ```
//!
//! Every chunk authenticates its position through its counter nonce and the
//! header through the AAD, so reordering, truncation, appending and header
//! tampering all fail authentication. The chunk count is fixed by
//! `payload.plainSize`, which also detects a silently missing tail.

use std::io::{Read, Write};

use chacha20poly1305::aead::{AeadInOut, Nonce};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::crypto::KEY_LEN;
use crate::crypto::header::VaultHeader;
use crate::error::AppError;

pub const MAGIC: &[u8; 8] = b"COFFEX01";
/// Container format version this build writes and accepts.
pub const FORMAT_VERSION: u32 = 1;
pub const KIND_BACKUP: &str = "backup";
pub const ALG_XCHACHA_CHUNKED: &str = "xchacha20poly1305-chunked";
pub const CODEC_VAULT_SNAPSHOT: &str = "vault-snapshot";

pub const CHUNK_SIZE: usize = 65_536;
pub const TAG_LEN: usize = 16;
pub const NONCE_LEN: usize = 24;
pub const NONCE_PREFIX_LEN: usize = 20;
pub const ARCHIVE_SALT_LEN: usize = 32;
/// Upper bound on the JSON header.
pub const MAX_HEADER_LEN: usize = 64 * 1024;
/// Upper bound on the decrypted payload.
pub const MAX_PLAIN: u64 = 4 * 1024 * 1024 * 1024;
/// Upper bound on the manifest entry.
pub const MAX_MANIFEST_LEN: usize = 1024 * 1024;

/// Largest on-disk `.coffer` accepted: header plus one tag per chunk.
pub const MAX_FILE_SIZE: u64 = MAX_PLAIN + (MAX_PLAIN / CHUNK_SIZE as u64 + 1) * TAG_LEN as u64;

pub const ENTRY_MANIFEST: u8 = 1;
pub const ENTRY_DB: u8 = 2;

const BACKUP_KEY_INFO: &[u8] = b"coffer/backup/v1";
const ARCHIVE_KEY_INFO: &[u8] = b"coffer/archive/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContainerHeader {
    pub format_version: u32,
    pub kind: String,
    pub created_at_ms: i64,
    pub source_vault_id: String,
    pub app_version: String,
    pub archive_salt_hex: String,
    pub cipher: CipherParams,
    pub payload: PayloadParams,
    pub vault_header: VaultHeader,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CipherParams {
    pub alg: String,
    pub nonce_prefix_hex: String,
    pub chunk_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PayloadParams {
    pub codec: String,
    pub plain_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: i64,
    pub app_version: String,
    pub source_vault_id: String,
    pub created_at_ms: i64,
    pub counts: ManifestCounts,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManifestCounts {
    pub items: i64,
    pub projects: i64,
    pub tags: i64,
}

pub fn unsupported(what: &str) -> AppError {
    AppError::unsupported_format(what)
}

/// Derives the backup key from the master key. A distinct info string keeps
/// the archive domain separate from SQLCipher's DB key.
pub fn derive_backup_key(mk: &[u8; KEY_LEN]) -> Result<Zeroizing<[u8; KEY_LEN]>, AppError> {
    let hk = Hkdf::<Sha256>::new(None, mk);
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    hk.expand(BACKUP_KEY_INFO, out.as_mut())
        .map_err(|_| AppError::internal("hkdf failure"))?;
    Ok(out)
}

/// Derives the archive key from the backup key and the archive's salt.
pub fn derive_archive_key(
    backup_key: &[u8; KEY_LEN],
    salt: &[u8; ARCHIVE_SALT_LEN],
) -> Result<Zeroizing<[u8; KEY_LEN]>, AppError> {
    let hk = Hkdf::<Sha256>::new(Some(salt), backup_key);
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    hk.expand(ARCHIVE_KEY_INFO, out.as_mut())
        .map_err(|_| AppError::internal("hkdf failure"))?;
    Ok(out)
}

fn decode_exact<const N: usize>(
    hex_value: &str,
    what: &str,
) -> Result<Zeroizing<[u8; N]>, AppError> {
    let raw = Zeroizing::new(hex::decode(hex_value).map_err(|_| unsupported(what))?);
    if raw.len() != N {
        return Err(unsupported(what));
    }
    let mut out = Zeroizing::new([0u8; N]);
    out.copy_from_slice(&raw);
    Ok(out)
}

impl ContainerHeader {
    /// Structural validation of every field and bound.
    pub fn validate(&self) -> Result<(), AppError> {
        if self.format_version != FORMAT_VERSION {
            return Err(unsupported("backup uses an unsupported format version"));
        }
        if self.kind != KIND_BACKUP {
            return Err(unsupported("unsupported backup kind"));
        }
        if self.cipher.alg != ALG_XCHACHA_CHUNKED {
            return Err(unsupported("unsupported backup cipher"));
        }
        if self.payload.codec != CODEC_VAULT_SNAPSHOT {
            return Err(unsupported("unsupported backup payload"));
        }
        if self.cipher.chunk_size as usize != CHUNK_SIZE {
            return Err(unsupported("unsupported backup chunk size"));
        }
        if self.payload.plain_size == 0 || self.payload.plain_size > MAX_PLAIN {
            return Err(unsupported("backup payload size is out of range"));
        }
        if self.created_at_ms <= 0 {
            return Err(unsupported("backup header is malformed"));
        }
        if self.source_vault_id != self.vault_header.vault_id {
            return Err(unsupported("backup header is inconsistent"));
        }
        self.archive_salt()?;
        self.nonce_prefix()?;
        self.vault_header
            .validate()
            .map_err(|_| unsupported("backup vault header is malformed"))
    }

    pub fn archive_salt(&self) -> Result<Zeroizing<[u8; ARCHIVE_SALT_LEN]>, AppError> {
        decode_exact(&self.archive_salt_hex, "backup salt is malformed")
    }

    pub fn nonce_prefix(&self) -> Result<Zeroizing<[u8; NONCE_PREFIX_LEN]>, AppError> {
        decode_exact(&self.cipher.nonce_prefix_hex, "backup nonce is malformed")
    }
}

/// 20-byte random prefix shared by every chunk nonce of one archive.
#[derive(Clone)]
pub struct NoncePrefix(Zeroizing<[u8; NONCE_PREFIX_LEN]>);

impl NoncePrefix {
    pub fn random() -> Self {
        let mut prefix = Zeroizing::new([0u8; NONCE_PREFIX_LEN]);
        crate::crypto::random_bytes(prefix.as_mut());
        Self(prefix)
    }

    pub fn from_bytes(bytes: [u8; NONCE_PREFIX_LEN]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub fn hex(&self) -> String {
        hex::encode(self.0.as_ref())
    }

    fn nonce(&self, counter: u32) -> [u8; NONCE_LEN] {
        let mut nonce = [0u8; NONCE_LEN];
        nonce[..NONCE_PREFIX_LEN].copy_from_slice(self.0.as_ref());
        nonce[NONCE_PREFIX_LEN..].copy_from_slice(&counter.to_be_bytes());
        nonce
    }
}

fn header_aad(header_bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(header_bytes).into()
}

fn cipher_for(key: &[u8; KEY_LEN]) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(key.into())
}

fn nonce(bytes: &[u8; NONCE_LEN]) -> Nonce<XChaCha20Poly1305> {
    Nonce::<XChaCha20Poly1305>::try_from(bytes.as_slice())
        .expect("NONCE_LEN equals the cipher's nonce size")
}

/// Serializes the header once; the same bytes are written and hashed into the
/// AAD so a modified header can never decrypt.
pub fn encode_header(header: &ContainerHeader) -> Result<Vec<u8>, AppError> {
    header.validate()?;
    let bytes = serde_json::to_vec(header).map_err(|_| AppError::internal("serialize"))?;
    if bytes.len() > MAX_HEADER_LEN {
        return Err(AppError::internal("backup header is too large"));
    }
    Ok(bytes)
}

/// Reads and validates the header from a reader positioned at offset 0.
/// Returns the parsed header and its exact on-disk bytes.
pub fn read_header<R: Read>(reader: &mut R) -> Result<(ContainerHeader, Vec<u8>), AppError> {
    let mut magic = [0u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|_| unsupported("not a Coffer backup file"))?;
    if &magic != MAGIC {
        return Err(unsupported("not a Coffer backup file"));
    }
    let mut len_buf = [0u8; 4];
    reader
        .read_exact(&mut len_buf)
        .map_err(|_| unsupported("backup header is truncated"))?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > MAX_HEADER_LEN {
        return Err(unsupported("backup header length is invalid"));
    }
    let mut bytes = vec![0u8; len];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| unsupported("backup header is truncated"))?;
    let header: ContainerHeader =
        serde_json::from_slice(&bytes).map_err(|_| unsupported("backup header is malformed"))?;
    header.validate()?;
    Ok((header, bytes))
}

/// Writes magic + header length + header.
pub fn write_header<W: Write>(writer: &mut W, header_bytes: &[u8]) -> Result<(), AppError> {
    if header_bytes.is_empty() || header_bytes.len() > MAX_HEADER_LEN {
        return Err(AppError::internal("backup header is too large"));
    }
    writer.write_all(MAGIC).map_err(AppError::io)?;
    writer
        .write_all(&(header_bytes.len() as u32).to_le_bytes())
        .map_err(AppError::io)?;
    writer.write_all(header_bytes).map_err(AppError::io)?;
    Ok(())
}

/// Encrypts a byte stream chunk by chunk into a writer.
pub struct ChunkEncryptor<W: Write> {
    writer: W,
    cipher: XChaCha20Poly1305,
    prefix: NoncePrefix,
    aad: [u8; 32],
    buffer: Vec<u8>,
    counter: u32,
    written: u64,
}

impl<W: Write> ChunkEncryptor<W> {
    pub fn new(
        writer: W,
        archive_key: &[u8; KEY_LEN],
        prefix: NoncePrefix,
        header_bytes: &[u8],
    ) -> Self {
        Self {
            writer,
            cipher: cipher_for(archive_key),
            prefix,
            aad: header_aad(header_bytes),
            buffer: Vec::with_capacity(CHUNK_SIZE),
            counter: 0,
            written: 0,
        }
    }

    /// Total plaintext bytes fed so far.
    pub fn written(&self) -> u64 {
        self.written
    }

    pub fn write_all(&mut self, mut data: &[u8]) -> Result<(), AppError> {
        self.written += data.len() as u64;
        while !data.is_empty() {
            let take = (CHUNK_SIZE - self.buffer.len()).min(data.len());
            self.buffer.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.buffer.len() == CHUNK_SIZE {
                self.flush_chunk()?;
            }
        }
        Ok(())
    }

    fn flush_chunk(&mut self) -> Result<(), AppError> {
        // Take the plaintext buffer so the ciphertext can be sealed into it in
        // place instead of allocating a second chunk-sized copy.
        let mut chunk = std::mem::replace(&mut self.buffer, Vec::with_capacity(CHUNK_SIZE));
        let chunk_nonce = nonce(&self.prefix.nonce(self.counter));
        let result = self
            .cipher
            .encrypt_in_place(&chunk_nonce, &self.aad, &mut chunk);
        if result.is_err() {
            zeroize::Zeroize::zeroize(&mut chunk);
            return Err(AppError::internal("backup encryption failed"));
        }
        self.counter = self
            .counter
            .checked_add(1)
            .ok_or_else(|| unsupported("backup is too large"))?;
        let written = self.writer.write_all(&chunk);
        zeroize::Zeroize::zeroize(&mut chunk);
        written.map_err(AppError::io)
    }

    /// Emits the final (possibly short) chunk and returns the writer.
    pub fn finish(mut self) -> Result<W, AppError> {
        if !self.buffer.is_empty() {
            self.flush_chunk()?;
        }
        Ok(self.writer)
    }
}

impl<W: Write> Write for ChunkEncryptor<W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.write_all(data)
            .map(|_| data.len())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

/// Incremental decryption of a payload written by [`ChunkEncryptor`]. Each
/// chunk is authenticated before any of its bytes are handed out.
pub struct ChunkReader<R: Read> {
    reader: R,
    cipher: XChaCha20Poly1305,
    prefix: NoncePrefix,
    aad: [u8; 32],
    plain_size: u64,
    chunk_count: u64,
    next: u64,
    delivered: u64,
    current: Zeroizing<Vec<u8>>,
    offset: usize,
}

impl<R: Read> ChunkReader<R> {
    pub fn new(
        reader: R,
        archive_key: &[u8; KEY_LEN],
        prefix: NoncePrefix,
        header_bytes: &[u8],
        plain_size: u64,
    ) -> Result<Self, AppError> {
        if plain_size == 0 || plain_size > MAX_PLAIN {
            return Err(unsupported("backup payload size is out of range"));
        }
        let chunk_count = plain_size.div_ceil(CHUNK_SIZE as u64);
        if chunk_count > u32::MAX as u64 {
            return Err(unsupported("backup has too many chunks"));
        }
        Ok(Self {
            reader,
            cipher: cipher_for(archive_key),
            prefix,
            aad: header_aad(header_bytes),
            plain_size,
            chunk_count,
            next: 0,
            delivered: 0,
            current: Zeroizing::new(Vec::new()),
            offset: 0,
        })
    }

    fn chunk_len(&self, index: u64) -> usize {
        let remaining = self.plain_size - index * CHUNK_SIZE as u64;
        remaining.min(CHUNK_SIZE as u64) as usize
    }

    fn load_chunk(&mut self) -> Result<bool, AppError> {
        if self.next >= self.chunk_count {
            return Ok(false);
        }
        let plain_len = self.chunk_len(self.next);
        let mut buf = vec![0u8; plain_len + TAG_LEN];
        self.reader
            .read_exact(&mut buf)
            .map_err(|_| unsupported("backup data is truncated"))?;
        let chunk_nonce = nonce(&self.prefix.nonce(self.next as u32));
        let opened = self
            .cipher
            .decrypt_in_place(&chunk_nonce, &self.aad, &mut buf);
        if opened.is_err() {
            zeroize::Zeroize::zeroize(&mut buf);
            // Wrong key, reordered chunk and bit flip are all indistinguishable
            // here; only password unwrap failures may report invalidPassword.
            return Err(unsupported("backup data is corrupted or was tampered with"));
        }
        self.next += 1;
        self.current = Zeroizing::new(buf);
        self.offset = 0;
        Ok(true)
    }

    /// Fills `out` with the next plaintext bytes; returns 0 at end of payload.
    /// Not named `read`, so a generic `R: Read` argument can never pick the
    /// infallible `Read::read` blanket implementation over this one.
    pub fn next_bytes(&mut self, out: &mut [u8]) -> Result<usize, AppError> {
        if out.is_empty() {
            return Ok(0);
        }
        if self.offset >= self.current.len() && !self.load_chunk()? {
            return Ok(0);
        }
        let n = (self.current.len() - self.offset).min(out.len());
        out[..n].copy_from_slice(&self.current[self.offset..self.offset + n]);
        self.offset += n;
        self.delivered += n as u64;
        if self.offset >= self.current.len() {
            self.current = Zeroizing::new(Vec::new());
        }
        Ok(n)
    }

    /// Reads the rest of the payload and proves the file ends exactly after the
    /// last authenticated chunk and that the decrypted size matched.
    pub fn finish(mut self) -> Result<(), AppError> {
        let mut scratch = [0u8; 8192];
        while self.next_bytes(&mut scratch)? > 0 {}
        if self.delivered != self.plain_size {
            return Err(unsupported("backup payload size does not match its header"));
        }
        let mut trailing = [0u8; 1];
        match self.reader.read(&mut trailing) {
            Ok(0) => Ok(()),
            Ok(_) => Err(unsupported("backup has trailing data")),
            Err(e) => Err(AppError::io(e)),
        }
    }
}

/// Total plaintext size of one framed entry header plus body.
pub const ENTRY_HEADER_LEN: u64 = 5;

/// Plaintext size of the payload framing `(kind, len)` pairs, given the DB
/// entry size and the serialized manifest length.
pub fn framed_plain_size(manifest_len: usize, db_len: u64) -> Option<u64> {
    let manifest = u64::try_from(manifest_len).ok()?;
    let total = ENTRY_HEADER_LEN
        .checked_add(manifest)?
        .checked_add(ENTRY_HEADER_LEN)?
        .checked_add(db_len)?;
    (total > 0 && total <= MAX_PLAIN).then_some(total)
}

/// Writes the `kind | u32 LE len` frame for one entry.
pub fn write_entry_header<W: Write>(writer: &mut W, kind: u8, len: u64) -> Result<(), AppError> {
    let len = u32::try_from(len).map_err(|_| AppError::internal("backup entry is too large"))?;
    let mut frame = Zeroizing::new([kind, 0, 0, 0, 0]);
    frame[1..].copy_from_slice(&len.to_le_bytes());
    writer.write_all(frame.as_ref()).map_err(AppError::io)
}

/// Parses the next entry frame. Returns `None` at the end of the payload and
/// enforces ordering: manifest first, then exactly one DB entry.
pub fn read_entry_header<R: Read>(
    reader: &mut ChunkReader<R>,
    manifest_seen: bool,
    db_seen: bool,
) -> Result<Option<(u8, u64)>, AppError> {
    let mut head = [0u8; 5];
    let mut got = 0;
    // End of payload may coincide with an entry boundary.
    while got < head.len() {
        // I/O errors must not be mistaken for end of stream, or a decryption
        // failure could look like a clean, complete payload.
        let n = reader.next_bytes(&mut head[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    if got == 0 {
        return Ok(None);
    }
    if got < head.len() {
        return Err(unsupported("backup payload is malformed"));
    }
    let kind = head[0];
    let len = u32::from_le_bytes([head[1], head[2], head[3], head[4]]) as u64;
    let accepted = match kind {
        ENTRY_MANIFEST => !manifest_seen && !db_seen && len <= MAX_MANIFEST_LEN as u64,
        ENTRY_DB => manifest_seen && !db_seen,
        _ => false,
    };
    if !accepted {
        return Err(unsupported("backup payload has unexpected entries"));
    }
    Ok(Some((kind, len)))
}

/// Decrypts the payload and hands each entry to `on_entry` as
/// `(kind, bounded_reader)`; the reader yields exactly that entry's bytes.
/// Trailing bytes, missing entries and a plainSize mismatch all fail.
pub fn decrypt<R: Read>(
    reader: R,
    header: &ContainerHeader,
    header_bytes: &[u8],
    archive_key: &[u8; KEY_LEN],
    mut on_progress: impl FnMut(u64, u64),
    mut on_entry: impl FnMut(u8, &mut Limited<'_, R>) -> Result<(), AppError>,
) -> Result<(), AppError> {
    let prefix = NoncePrefix::from_bytes(*header.nonce_prefix()?);
    let mut stream = ChunkReader::new(
        reader,
        archive_key,
        prefix,
        header_bytes,
        header.payload.plain_size,
    )?;
    let mut seen_manifest = false;
    let mut seen_db = false;
    while let Some((kind, len)) = read_entry_header(&mut stream, seen_manifest, seen_db)? {
        if len > stream.plain_size - stream.delivered {
            return Err(unsupported("backup payload is malformed"));
        }
        on_progress(stream.delivered, stream.plain_size);
        let mut limited = Limited {
            inner: &mut stream,
            remaining: len,
        };
        on_entry(kind, &mut limited)?;
        if limited.remaining != 0 {
            return Err(AppError::internal("backup entry was not fully read"));
        }
        if kind == ENTRY_MANIFEST {
            seen_manifest = true;
        } else {
            seen_db = true;
        }
        on_progress(stream.delivered, stream.plain_size);
    }
    if !seen_manifest || !seen_db {
        return Err(unsupported("backup payload is missing required entries"));
    }
    stream.finish()
}

/// Read adapter that stops at the end of one entry, so a handler cannot read
/// across entry boundaries.
///
/// `ChunkReader` deliberately does not implement `std::io::Read`: the blanket
/// `Read for &mut R` impl would win over `&mut ChunkReader` in a generic
/// context and recurse, hiding AEAD failures behind endless zero-length reads.
pub struct Limited<'a, R: Read> {
    inner: &'a mut ChunkReader<R>,
    remaining: u64,
}

impl<R: Read> Limited<'_, R> {
    /// Reads up to `buf.len()` bytes of the current entry; 0 at its end.
    pub fn read_some(&mut self, buf: &mut [u8]) -> Result<usize, AppError> {
        if self.remaining == 0 || buf.is_empty() {
            return Ok(0);
        }
        let want = buf.len().min(self.remaining as usize);
        let n = self.inner.next_bytes(&mut buf[..want])?;
        self.remaining -= n as u64;
        Ok(n)
    }

    pub fn read_all(&mut self) -> Result<Zeroizing<Vec<u8>>, AppError> {
        let mut out = Zeroizing::new(Vec::with_capacity(self.remaining as usize));
        let mut scratch = Zeroizing::new(vec![0u8; CHUNK_SIZE]);
        loop {
            let n = self.read_some(&mut scratch)?;
            if n == 0 {
                break;
            }
            out.extend_from_slice(&scratch[..n]);
        }
        Ok(out)
    }

    /// Copies the entry into `sink` in bounded slices.
    pub fn copy_to(&mut self, sink: &mut impl Write) -> Result<(), AppError> {
        let mut scratch = Zeroizing::new(vec![0u8; CHUNK_SIZE]);
        loop {
            let n = self.read_some(&mut scratch)?;
            if n == 0 {
                return Ok(());
            }
            sink.write_all(&scratch[..n]).map_err(AppError::io)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;
    use std::io::Cursor;

    fn sample_vault_header() -> VaultHeader {
        VaultHeader {
            version: 1,
            vault_id: "0199aaaa-bbbb-7ccc-8ddd-eeeeffff0000".into(),
            kdf: "argon2id".into(),
            m_cost: 65_536,
            t_cost: 3,
            p_cost: 1,
            salt_hex: hex::encode([1u8; 32]),
            wrap_nonce_hex: hex::encode([2u8; 24]),
            wrapped_mk_hex: hex::encode([3u8; 48]),
        }
    }

    fn sample_manifest() -> Manifest {
        Manifest {
            schema_version: 1,
            app_version: "0.1.0".into(),
            source_vault_id: sample_vault_header().vault_id,
            created_at_ms: 1_790_000_000_000,
            counts: ManifestCounts {
                items: 2,
                projects: 1,
                tags: 3,
            },
        }
    }

    fn db_bytes(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 31 % 251) as u8).collect()
    }

    pub(super) struct Archive {
        pub(super) header: ContainerHeader,
        pub(super) header_bytes: Vec<u8>,
        pub(super) key: Zeroizing<[u8; KEY_LEN]>,
        pub(super) file: Vec<u8>,
        pub(super) manifest: Manifest,
        pub(super) db: Vec<u8>,
    }

    fn header_for(
        prefix: &NoncePrefix,
        salt: &[u8; ARCHIVE_SALT_LEN],
        plain_size: u64,
    ) -> ContainerHeader {
        ContainerHeader {
            format_version: FORMAT_VERSION,
            kind: KIND_BACKUP.into(),
            created_at_ms: 1_790_000_000_000,
            source_vault_id: sample_vault_header().vault_id,
            app_version: "0.1.0".into(),
            archive_salt_hex: hex::encode(salt),
            cipher: CipherParams {
                alg: ALG_XCHACHA_CHUNKED.into(),
                nonce_prefix_hex: prefix.hex(),
                chunk_size: CHUNK_SIZE as u32,
            },
            payload: PayloadParams {
                codec: CODEC_VAULT_SNAPSHOT.into(),
                plain_size,
            },
            vault_header: sample_vault_header(),
        }
    }

    /// Writes a container of the given entries without checking that
    /// plainSize matches their framing (used to build malformed archives).
    fn raw_archive(
        header: &ContainerHeader,
        key: &[u8; KEY_LEN],
        entries: &[(u8, &[u8])],
    ) -> Vec<u8> {
        let header_bytes = encode_header(header).unwrap();
        let prefix =
            NoncePrefix::from_bytes(header.nonce_prefix().unwrap().as_ref().try_into().unwrap());
        let mut out = Vec::new();
        write_header(&mut out, &header_bytes).unwrap();
        let mut enc = ChunkEncryptor::new(&mut out, key, prefix, &header_bytes);
        for (kind, body) in entries {
            let mut frame = Vec::new();
            write_entry_header(&mut frame, *kind, body.len() as u64).unwrap();
            enc.write_all(&frame).unwrap();
            enc.write_all(body).unwrap();
        }
        enc.finish().unwrap();
        out
    }

    fn build_with(db: &[u8]) -> Archive {
        let salt = [9u8; ARCHIVE_SALT_LEN];
        let prefix = NoncePrefix::random();
        let manifest = sample_manifest();
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let plain_size = framed_plain_size(manifest_bytes.len(), db.len() as u64).unwrap();
        let header = header_for(&prefix, &salt, plain_size);
        let header_bytes = encode_header(&header).unwrap();
        let key = derive_archive_key(&[7u8; KEY_LEN], &salt).unwrap();
        let file = raw_archive(
            &header,
            &key,
            &[(ENTRY_MANIFEST, &manifest_bytes), (ENTRY_DB, db)],
        );
        Archive {
            header,
            header_bytes,
            key,
            file,
            manifest,
            db: db.to_vec(),
        }
    }

    pub(super) fn build() -> Archive {
        build_with(&db_bytes(200_000))
    }

    /// Decrypts and returns (manifest bytes, db bytes).
    pub(super) fn open(file: &[u8], key: &[u8; KEY_LEN]) -> Result<(Vec<u8>, Vec<u8>), AppError> {
        let mut cursor = Cursor::new(file);
        let (header, header_bytes) = read_header(&mut cursor)?;
        let mut manifest = None;
        let mut db = Vec::new();
        decrypt(
            cursor,
            &header,
            &header_bytes,
            key,
            |_, _| {},
            |kind, entry| {
                if kind == ENTRY_MANIFEST {
                    manifest = Some(entry.read_all()?.to_vec());
                } else {
                    let mut sink = Vec::new();
                    entry.copy_to(&mut sink)?;
                    db.extend_from_slice(&sink);
                }
                Ok(())
            },
        )?;
        Ok((manifest.unwrap_or_default(), db))
    }

    /// Tampered archives must fail with `unsupportedFormat`, never decrypt.
    fn rejected(result: Result<(Vec<u8>, Vec<u8>), AppError>) -> bool {
        matches!(result, Err(e) if e.code == ErrorCode::UnsupportedFormat)
    }

    #[test]
    fn round_trip_is_byte_identical() {
        let a = build();
        let (manifest, db) = open(&a.file, &a.key).unwrap();
        assert_eq!(
            serde_json::from_slice::<Manifest>(&manifest).unwrap(),
            a.manifest
        );
        assert_eq!(db, a.db);
        assert!(!a.file.starts_with(b"SQLite format 3"));
    }

    #[test]
    fn boundary_sizes_round_trip() {
        // Manifest frame is 5 + len bytes, so payloads land on chunk edges here.
        for len in [0usize, 1, CHUNK_SIZE - 200, CHUNK_SIZE + 1] {
            let a = build_with(&db_bytes(len));
            let (_, db) = open(&a.file, &a.key).unwrap();
            assert_eq!(db.len(), len);
        }
    }

    #[test]
    fn header_bytes_are_auth_bound() {
        let a = build();
        assert_eq!(a.header_bytes, &a.file[12..12 + a.header_bytes.len()]);
        assert!(read_header(&mut Cursor::new(&a.file)).is_ok());
    }

    #[test]
    fn wrong_key_fails_cleanly() {
        let a = build();
        assert!(rejected(open(&a.file, &[8u8; KEY_LEN])));
    }

    #[test]
    fn flipping_a_header_byte_fails() {
        let a = build();
        // Keep the JSON valid (flip one hex digit) so only the AAD changes.
        let needle = b"wrappedMkHex\":\"";
        let at = a
            .file
            .windows(needle.len())
            .position(|w| w == needle)
            .unwrap()
            + needle.len();
        let mut file = a.file.clone();
        file[at] = if file[at] == b'3' { b'4' } else { b'3' };
        assert!(rejected(open(&file, &a.key)));
    }

    #[test]
    fn flipping_a_ciphertext_byte_fails() {
        let a = build();
        let split = 12 + a.header_bytes.len();
        let mut file = a.file.clone();
        let mid = split + (file.len() - split) / 2;
        file[mid] ^= 0x01;
        assert!(rejected(open(&file, &a.key)));
    }

    #[test]
    fn truncation_fails() {
        let a = build();
        let split = 12 + a.header_bytes.len();
        let mut file = a.file.clone();
        file.truncate(file.len() - TAG_LEN - 1);
        assert!(rejected(open(&file, &a.key)));
        // Exactly one whole chunk removed, on a payload that has several.
        let mut file = a.file.clone();
        file.truncate(split + (CHUNK_SIZE + TAG_LEN) * 2);
        assert!(rejected(open(&file, &a.key)));
    }

    #[test]
    fn appended_bytes_fail() {
        let a = build();
        let mut file = a.file.clone();
        file.push(0);
        assert!(rejected(open(&file, &a.key)));
    }

    #[test]
    fn reordered_chunks_fail() {
        let a = build();
        let split = 12 + a.header_bytes.len();
        let step = CHUNK_SIZE + TAG_LEN;
        let mut file = a.file.clone();
        let body: Vec<u8> = file[split..].to_vec();
        file.truncate(split);
        file.extend_from_slice(&body[step..2 * step]);
        file.extend_from_slice(&body[..step]);
        file.extend_from_slice(&body[2 * step..]);
        assert!(rejected(open(&file, &a.key)));
    }

    #[test]
    fn bad_magic_and_oversized_header_fail() {
        let a = build();
        let mut file = a.file.clone();
        file[0] = b'X';
        assert!(rejected(open(&file, &a.key)));

        let mut file = a.file.clone();
        file[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            read_header(&mut Cursor::new(&file)),
            Err(e) if e.code == ErrorCode::UnsupportedFormat
        ));

        let mut file = a.file.clone();
        let too_big = (MAX_HEADER_LEN as u32 + 1).to_le_bytes();
        file[8..12].copy_from_slice(&too_big);
        assert!(matches!(
            read_header(&mut Cursor::new(&file)),
            Err(e) if e.code == ErrorCode::UnsupportedFormat
        ));

        // Header JSON longer than the truncated remainder.
        assert!(matches!(
            read_header(&mut Cursor::new(&a.file[..20])),
            Err(e) if e.code == ErrorCode::UnsupportedFormat
        ));
    }

    #[test]
    fn unknown_header_fields_and_versions_rejected() {
        let a = build();
        let mut value: serde_json::Value = serde_json::from_slice(&a.header_bytes).unwrap();
        value["extra"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ContainerHeader>(value).is_err());

        for mutate in [
            |h: &mut ContainerHeader| h.format_version = 2,
            |h: &mut ContainerHeader| h.kind = "portable".into(),
            |h: &mut ContainerHeader| h.cipher.alg = "aes-gcm".into(),
            |h: &mut ContainerHeader| h.cipher.chunk_size = 4096,
            |h: &mut ContainerHeader| h.payload.codec = "json".into(),
            |h: &mut ContainerHeader| h.payload.plain_size = 0,
            |h: &mut ContainerHeader| h.payload.plain_size = MAX_PLAIN + 1,
            |h: &mut ContainerHeader| h.archive_salt_hex = "00".into(),
            |h: &mut ContainerHeader| h.cipher.nonce_prefix_hex = hex::encode([0u8; 19]),
            |h: &mut ContainerHeader| h.created_at_ms = -1,
            |h: &mut ContainerHeader| h.vault_header.wrapped_mk_hex = "zz".into(),
        ] {
            let mut h = a.header.clone();
            mutate(&mut h);
            assert!(matches!(
                h.validate(),
                Err(e) if e.code == ErrorCode::UnsupportedFormat
            ));
        }
    }

    #[test]
    fn plain_size_mismatch_fails() {
        let a = build();
        let manifest_bytes = serde_json::to_vec(&a.manifest).unwrap();
        let entries = [
            (ENTRY_MANIFEST, manifest_bytes.as_slice()),
            (ENTRY_DB, a.db.as_slice()),
        ];
        for delta in [-9i64, 1] {
            let mut h = a.header.clone();
            let actual = framed_plain_size(manifest_bytes.len(), a.db.len() as u64).unwrap();
            h.payload.plain_size = (actual as i64 + delta) as u64;
            let file = raw_archive(&h, &a.key, &entries);
            assert!(
                rejected(open(&file, &a.key)),
                "plainSize delta {delta} must be rejected"
            );
        }
    }

    #[test]
    fn manifest_must_be_first() {
        let a = build();
        let manifest_bytes = serde_json::to_vec(&a.manifest).unwrap();
        let h = header_for(
            &NoncePrefix::from_bytes(*a.header.nonce_prefix().unwrap()),
            &[9u8; ARCHIVE_SALT_LEN],
            framed_plain_size(manifest_bytes.len(), a.db.len() as u64).unwrap(),
        );
        let file = raw_archive(
            &h,
            &a.key,
            &[(ENTRY_DB, &a.db), (ENTRY_MANIFEST, &manifest_bytes)],
        );
        assert!(rejected(open(&file, &a.key)));
    }

    #[test]
    fn key_derivation_is_domain_separated() {
        let mk = [4u8; KEY_LEN];
        let backup = derive_backup_key(&mk).unwrap();
        assert_ne!(backup.as_ref(), &mk);
        assert_ne!(
            backup.as_ref(),
            crate::crypto::derive_db_key(&mk).unwrap().as_ref()
        );
        let a = derive_archive_key(&backup, &[1u8; ARCHIVE_SALT_LEN]).unwrap();
        let b = derive_archive_key(&backup, &[2u8; ARCHIVE_SALT_LEN]).unwrap();
        assert_ne!(a.as_ref(), b.as_ref());
        assert_ne!(a.as_ref(), backup.as_ref());
    }
}
