//! Bounded file messages and explicit local-destination storage.
//!
//! A caller must hold a live `RemotePermission::Files` session, send these
//! messages only through `Channel::File`, and wait for each `ChunkAck` before
//! sending the next chunk. Receiving an offer does not create a file. The
//! receiver is created only after a local user selects a destination. Dropping
//! it on cancellation, authorization loss, or disconnect removes its partial.
#[cfg(any(unix, windows))]
use cap_std::fs::OpenOptionsExt;
use cap_std::{
    ambient_authority,
    fs::{Dir, File as CapFile, OpenOptions},
};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub const CHUNK_SIZE: usize = 64 * 1024;
pub const MAX_METADATA_SIZE: usize = 4096;
pub const MAX_FILE_SIZE: u64 = 64 * 1024 * 1024 * 1024;
pub const MAX_FRAME_SIZE: usize = CHUNK_SIZE + 29;
const MAGIC: &[u8; 4] = b"FSF1";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid file transfer message")]
    Invalid,
    #[error("file transfer exceeds its limit")]
    Limit,
    #[error("file chunk is out of sequence")]
    Sequence,
    #[error("file content failed integrity verification")]
    Integrity,
    #[error("destination already exists")]
    Exists,
    #[error("file transfer was cancelled")]
    Cancelled,
    #[error("file operation failed")]
    Io(#[source] std::io::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            Self::Exists
        } else {
            Self::Io(error)
        }
    }
}

/// These are display names, never paths. Use the same rules on both platforms.
pub fn valid_name(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 255
        || matches!(name, "." | "..")
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
    {
        return false;
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        // Windows additionally reserves superscript digits in these names.
        && !matches!(stem.as_str(), "COM¹" | "COM²" | "COM³" | "LPT¹" | "LPT²" | "LPT³")
}

fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|x| x.is_ascii_digit() || (b'a'..=b'f').contains(&x))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub sha256: String,
}
impl Offer {
    pub fn validate(&self) -> Result<()> {
        if !valid_id(&self.id)
            || !valid_name(&self.name)
            || self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|x| x.is_ascii_digit() || (b'a'..=b'f').contains(&x))
        {
            return Err(Error::Invalid);
        }
        if self.size > MAX_FILE_SIZE {
            return Err(Error::Limit);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    Declined,
    Busy,
    Invalid,
    Io,
    Integrity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Offer(Offer),
    Accept {
        id: String,
    },
    Reject {
        id: String,
        reason: RejectReason,
    },
    Chunk {
        id: String,
        offset: u64,
        data: Vec<u8>,
    },
    ChunkAck {
        id: String,
        offset: u64,
    },
    Finish {
        id: String,
    },
    Complete {
        id: String,
    },
    Cancel {
        id: String,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Metadata {
    Offer { offer: Offer },
    Accept { id: String },
    Reject { id: String, reason: RejectReason },
    ChunkAck { id: String, offset: u64 },
    Finish { id: String },
    Complete { id: String },
    Cancel { id: String },
}

impl Message {
    pub fn id(&self) -> &str {
        match self {
            Self::Offer(offer) => &offer.id,
            Self::Accept { id }
            | Self::Reject { id, .. }
            | Self::Chunk { id, .. }
            | Self::ChunkAck { id, .. }
            | Self::Finish { id }
            | Self::Complete { id }
            | Self::Cancel { id } => id,
        }
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        if !valid_id(self.id()) {
            return Err(Error::Invalid);
        }
        let mut out = MAGIC.to_vec();
        if let Self::Chunk { id, offset, data } = self {
            if data.is_empty() || data.len() > CHUNK_SIZE || *offset > MAX_FILE_SIZE {
                return Err(Error::Limit);
            }
            out.push(1);
            out.extend_from_slice(&hex::decode(id).map_err(|_| Error::Invalid)?);
            out.extend_from_slice(&offset.to_be_bytes());
            out.extend_from_slice(data);
            return Ok(out);
        }
        let metadata = match self {
            Self::Offer(offer) => {
                offer.validate()?;
                Metadata::Offer {
                    offer: offer.clone(),
                }
            }
            Self::Accept { id } => Metadata::Accept { id: id.clone() },
            Self::Reject { id, reason } => Metadata::Reject {
                id: id.clone(),
                reason: *reason,
            },
            Self::ChunkAck { id, offset } => {
                if *offset > MAX_FILE_SIZE {
                    return Err(Error::Limit);
                }
                Metadata::ChunkAck {
                    id: id.clone(),
                    offset: *offset,
                }
            }
            Self::Finish { id } => Metadata::Finish { id: id.clone() },
            Self::Complete { id } => Metadata::Complete { id: id.clone() },
            Self::Cancel { id } => Metadata::Cancel { id: id.clone() },
            Self::Chunk { .. } => unreachable!(),
        };
        out.push(0);
        let json = serde_json::to_vec(&metadata).map_err(|_| Error::Invalid)?;
        if json.len() > MAX_METADATA_SIZE {
            return Err(Error::Limit);
        }
        out.extend_from_slice(&json);
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 5 || &bytes[..4] != MAGIC {
            return Err(Error::Invalid);
        }
        let message = match bytes[4] {
            0 => {
                if bytes.len() - 5 > MAX_METADATA_SIZE {
                    return Err(Error::Limit);
                }
                let metadata: Metadata =
                    serde_json::from_slice(&bytes[5..]).map_err(|_| Error::Invalid)?;
                match metadata {
                    Metadata::Offer { offer } => {
                        offer.validate()?;
                        Self::Offer(offer)
                    }
                    Metadata::Accept { id } => Self::Accept { id },
                    Metadata::Reject { id, reason } => Self::Reject { id, reason },
                    Metadata::ChunkAck { id, offset } => {
                        if offset > MAX_FILE_SIZE {
                            return Err(Error::Limit);
                        }
                        Self::ChunkAck { id, offset }
                    }
                    Metadata::Finish { id } => Self::Finish { id },
                    Metadata::Complete { id } => Self::Complete { id },
                    Metadata::Cancel { id } => Self::Cancel { id },
                }
            }
            1 => {
                if bytes.len() <= 29 {
                    return Err(Error::Invalid);
                }
                if bytes.len() > MAX_FRAME_SIZE {
                    return Err(Error::Limit);
                }
                let offset =
                    u64::from_be_bytes(bytes[21..29].try_into().map_err(|_| Error::Invalid)?);
                if offset > MAX_FILE_SIZE {
                    return Err(Error::Limit);
                }
                Self::Chunk {
                    id: hex::encode(&bytes[5..21]),
                    offset,
                    data: bytes[29..].to_vec(),
                }
            }
            _ => return Err(Error::Invalid),
        };
        if !valid_id(message.id()) {
            return Err(Error::Invalid);
        }
        Ok(message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    Offered,
    Waiting,
    Sending,
    Receiving,
    Completed,
    Cancelled,
    Rejected,
    Failed,
}

/// Public progress does not contain source/destination paths or credentials.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub transferred: u64,
    pub state: TransferState,
}

pub struct Sender {
    file: File,
    offer: Offer,
    transferred: u64,
    digest: Sha256,
    checked: bool,
}
impl Sender {
    /// This performs blocking disk IO; call from a blocking worker, not an async
    /// executor thread. Keep the chosen path inside the native boundary.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_cancellable(path, &AtomicBool::new(false))
    }
    pub fn open_cancellable(path: impl AsRef<Path>, cancelled: &AtomicBool) -> Result<Self> {
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        let path = path.as_ref();
        let name = path
            .file_name()
            .and_then(|x| x.to_str())
            .ok_or(Error::Invalid)?;
        if !valid_name(name) {
            return Err(Error::Invalid);
        }
        // Stat before opening catches explicitly selected devices/FIFOs. The
        // nonblocking flag also closes a replacement-to-FIFO race between stat
        // and open; it has no effect on reads from ordinary disk files.
        if !std::fs::metadata(path)?.is_file() {
            return Err(Error::Invalid);
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let mut file = options.open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(Error::Invalid);
        }
        if metadata.len() > MAX_FILE_SIZE {
            return Err(Error::Limit);
        }
        let mut digest = Sha256::new();
        let mut buffer = vec![0; CHUNK_SIZE];
        let mut total = 0u64;
        loop {
            if cancelled.load(Ordering::Acquire) {
                return Err(Error::Cancelled);
            }
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            total = total.checked_add(n as u64).ok_or(Error::Limit)?;
            if total > metadata.len() {
                return Err(Error::Integrity);
            }
            digest.update(&buffer[..n]);
        }
        if total != metadata.len() {
            return Err(Error::Integrity);
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        file.seek(SeekFrom::Start(0))?;
        let mut random_id = [0u8; 16];
        OsRng.fill_bytes(&mut random_id);
        let offer = Offer {
            id: hex::encode(random_id),
            name: name.to_owned(),
            size: total,
            sha256: hex::encode(digest.finalize()),
        };
        Ok(Self {
            file,
            offer,
            transferred: 0,
            digest: Sha256::new(),
            checked: false,
        })
    }
    pub fn offer(&self) -> &Offer {
        &self.offer
    }
    pub fn transferred(&self) -> u64 {
        self.transferred
    }
    /// `None` means the originally offered content hash has been rechecked and
    /// a Finish may be sent. A source changed mid-transfer is never completed.
    pub fn next_chunk(&mut self) -> Result<Option<Message>> {
        if self.checked {
            return Ok(None);
        }
        let mut data = vec![0; CHUNK_SIZE];
        let count = self.file.read(&mut data)?;
        if count == 0 {
            if self.transferred != self.offer.size
                || hex::encode(self.digest.clone().finalize()) != self.offer.sha256
            {
                return Err(Error::Integrity);
            }
            self.checked = true;
            return Ok(None);
        }
        let next = self
            .transferred
            .checked_add(count as u64)
            .ok_or(Error::Limit)?;
        if next > self.offer.size {
            return Err(Error::Integrity);
        }
        data.truncate(count);
        self.digest.update(&data);
        let message = Message::Chunk {
            id: self.offer.id.clone(),
            offset: self.transferred,
            data,
        };
        self.transferred = next;
        Ok(Some(message))
    }
}

pub struct Receiver {
    offer: Offer,
    destination: PathBuf,
    destination_name: OsString,
    partial: Partial,
    transferred: u64,
    digest: Sha256,
}

/// Keep all writes, publication and cleanup anchored to the directory selected
/// at acceptance, even if an ancestor is renamed or replaced while transferring.
struct Partial {
    dir: Dir,
    name: String,
    file: Option<CapFile>,
}
impl Drop for Partial {
    fn drop(&mut self) {
        // Windows may refuse unlinking while the write handle is still open.
        drop(self.file.take());
        let _ = self.dir.remove_file(&self.name);
    }
}
impl Receiver {
    /// No destination is derived from peer metadata. The native user chooses
    /// this exact absolute path through a save dialog before acceptance.
    pub fn accept(offer: Offer, destination: impl AsRef<Path>) -> Result<Self> {
        offer.validate()?;
        let destination = destination.as_ref();
        if !destination.is_absolute() || destination.file_name().is_none() {
            return Err(Error::Invalid);
        }
        let parent = destination.parent().ok_or(Error::Invalid)?;
        let destination_name = destination.file_name().ok_or(Error::Invalid)?.to_owned();
        let dir = Dir::open_ambient_dir(parent, ambient_authority())?;
        // Query through the selected directory capability, never reopen a path.
        // symlink_metadata rejects dangling symlinks as well as normal files.
        match dir.symlink_metadata(&destination_name) {
            Ok(_) => return Err(Error::Exists),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        #[cfg(windows)]
        options.share_mode(1); // FILE_SHARE_READ only: retain the partial's identity.
        let mut random = [0; 16];
        OsRng.fill_bytes(&mut random);
        let name = format!(".farsail-part-{}", hex::encode(random));
        let file = dir.open_with(&name, &options)?;
        let partial = Partial {
            dir,
            name,
            file: Some(file),
        };
        Ok(Self {
            offer,
            destination: destination.to_owned(),
            destination_name,
            partial,
            transferred: 0,
            digest: Sha256::new(),
        })
    }
    pub fn offer(&self) -> &Offer {
        &self.offer
    }
    pub fn transferred(&self) -> u64 {
        self.transferred
    }
    pub fn write_chunk(&mut self, id: &str, offset: u64, data: &[u8]) -> Result<u64> {
        if id != self.offer.id || offset != self.transferred {
            return Err(Error::Sequence);
        }
        if data.is_empty() || data.len() > CHUNK_SIZE {
            return Err(Error::Limit);
        }
        let next = self
            .transferred
            .checked_add(data.len() as u64)
            .ok_or(Error::Limit)?;
        if next > self.offer.size {
            return Err(Error::Limit);
        }
        self.partial
            .file
            .as_mut()
            .ok_or(Error::Invalid)?
            .write_all(data)?;
        self.digest.update(data);
        self.transferred = next;
        Ok(next)
    }
    /// Consume the receiver, check the complete digest, and publish atomically
    /// without replacing any file created since acceptance. Failure drops the
    /// partial as well. Send Complete only after this call succeeds.
    pub fn finish(self) -> Result<PathBuf> {
        self.finish_guarded(|| Ok(()))
    }
    /// Acquire a local lifecycle guard immediately before publishing. The
    /// caller marks cancellation while holding the same guard, and checks its
    /// session's lease and cancellation token while acquiring it. Disk flush
    /// happens before acquisition; G remains held across the final link.
    pub fn finish_guarded<G>(self, acquire: impl FnOnce() -> Result<G>) -> Result<PathBuf> {
        self.finish_guarded_with(acquire, || {})
    }
    /// The callback runs after publication while G is still held, so a runtime
    /// can atomically mark the completed transfer before allowing cancellation.
    pub fn finish_guarded_with<G>(
        mut self,
        acquire: impl FnOnce() -> Result<G>,
        committed: impl FnOnce(),
    ) -> Result<PathBuf> {
        if self.transferred != self.offer.size
            || hex::encode(self.digest.clone().finalize()) != self.offer.sha256
        {
            return Err(Error::Integrity);
        }
        let file = self.partial.file.as_mut().ok_or(Error::Invalid)?;
        file.flush()?;
        file.sync_all()?;
        let _guard = acquire()?;
        // Hard-link creation is atomic and fails if any destination entry
        // exists. Both names resolve through the already-open directory. On
        // filesystems without hard links, fail closed rather than use a racy
        // rename/copy fallback. Only the completed, verified inode is published.
        self.partial.dir.hard_link(
            &self.partial.name,
            &self.partial.dir,
            &self.destination_name,
        )?;
        committed();
        drop(self.partial.file.take());
        // Drop retries cleanup if the first attempt encounters a transient
        // failure. The published file is already complete and remains valid.
        let _ = self.partial.dir.remove_file(&self.partial.name);
        Ok(self.destination)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn source(dir: &Path, bytes: &[u8]) -> PathBuf {
        let source = dir.join("原始文件.txt");
        fs::write(&source, bytes).unwrap();
        source
    }
    fn empty_offer() -> Offer {
        Offer {
            id: "01".repeat(16),
            name: "example.txt".into(),
            size: 0,
            sha256: hex::encode(Sha256::digest([])),
        }
    }
    #[tokio::test]
    async fn authenticated_p2p_file_session_roundtrips_and_keeps_permission_exact() {
        use farsail_core::RemotePermission;
        use farsail_transport::{Authority, Channel, Claims, Config, Frame, Session, Transport};
        use std::{sync::Arc, time::Instant};
        struct FileAuthority {
            source: String,
            target: String,
        }
        impl Authority for FileAuthority {
            async fn inspect(&self, id: &str, token: &str) -> farsail_transport::Result<Claims> {
                if id != "files-test" || token != "files-grant" {
                    return Err(farsail_transport::Error::Denied);
                }
                Ok(Claims {
                    session_id: id.into(),
                    permission: RemotePermission::Files,
                    source_public_key: self.source.clone(),
                    target_public_key: self.target.clone(),
                    nonce: "01".repeat(32),
                    expires_in: 10,
                    checked_at: Instant::now(),
                })
            }
            async fn issue(&self, id: &str, _: bool) -> farsail_transport::Result<String> {
                if id != "files-test" {
                    return Err(farsail_transport::Error::Denied);
                }
                Ok("files-grant".into())
            }
        }
        async fn exchange(from: &Session, to: &Session, message: Message) -> Message {
            from.send(Frame {
                channel: Channel::File,
                bytes: message.encode().unwrap(),
            })
            .await
            .unwrap();
            let frame = to.receive().await.unwrap();
            assert_eq!(frame.channel, Channel::File);
            Message::decode(&frame.bytes).unwrap()
        }
        let source_endpoint = Transport::bind([71; 32], Config::default()).await.unwrap();
        let target_endpoint = Transport::bind([72; 32], Config::default()).await.unwrap();
        let authority = Arc::new(FileAuthority {
            source: hex::encode(source_endpoint.id().as_bytes()),
            target: hex::encode(target_endpoint.id().as_bytes()),
        });
        let host = tokio::spawn({
            let target = target_endpoint.clone();
            let authority = authority.clone();
            async move { target.accept(authority).await.unwrap() }
        });
        let outgoing = source_endpoint
            .connect(
                target_endpoint.addr(),
                "files-test",
                RemotePermission::Files,
                authority,
            )
            .await
            .unwrap();
        let incoming = host.await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let bytes = vec![171; CHUNK_SIZE * 2 + 111];
        let mut sender = Sender::open(source(dir.path(), &bytes)).unwrap();
        let Message::Offer(offer) =
            exchange(&outgoing, &incoming, Message::Offer(sender.offer().clone())).await
        else {
            panic!()
        };
        let id = offer.id.clone();
        let destination = dir.path().join("p2p-received.txt");
        let mut receiver = Receiver::accept(offer, &destination).unwrap();
        assert_eq!(
            exchange(&incoming, &outgoing, Message::Accept { id: id.clone() }).await,
            Message::Accept { id: id.clone() }
        );
        while let Some(chunk) = sender.next_chunk().unwrap() {
            let Message::Chunk {
                id: received_id,
                offset,
                data,
            } = exchange(&outgoing, &incoming, chunk).await
            else {
                panic!()
            };
            let next = receiver.write_chunk(&received_id, offset, &data).unwrap();
            assert_eq!(
                exchange(
                    &incoming,
                    &outgoing,
                    Message::ChunkAck {
                        id: id.clone(),
                        offset: next
                    }
                )
                .await,
                Message::ChunkAck {
                    id: id.clone(),
                    offset: sender.transferred()
                }
            );
        }
        assert_eq!(
            exchange(&outgoing, &incoming, Message::Finish { id: id.clone() }).await,
            Message::Finish { id: id.clone() }
        );
        receiver.finish().unwrap();
        assert_eq!(
            exchange(&incoming, &outgoing, Message::Complete { id: id.clone() }).await,
            Message::Complete { id }
        );
        assert_eq!(fs::read(destination).unwrap(), bytes);
        assert_eq!(outgoing.path().0, "direct");
        assert!(matches!(
            outgoing
                .send(Frame {
                    channel: Channel::Media,
                    bytes: vec![1]
                })
                .await,
            Err(farsail_transport::Error::Denied)
        ));
        outgoing.close();
        incoming.close();
        source_endpoint.close().await;
        target_endpoint.close().await;
    }
    #[test]
    fn roundtrip_chunks_verify_before_publishing() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = (0..CHUNK_SIZE * 3 + 173)
            .map(|x| (x % 251) as u8)
            .collect::<Vec<_>>();
        let mut sender = Sender::open(source(dir.path(), &bytes)).unwrap();
        let wire_offer = Message::Offer(sender.offer().clone()).encode().unwrap();
        let Message::Offer(offer) = Message::decode(&wire_offer).unwrap() else {
            panic!()
        };
        let destination = dir.path().join("received.txt");
        let mut receiver = Receiver::accept(offer, &destination).unwrap();
        let mut count = 0;
        while let Some(message) = sender.next_chunk().unwrap() {
            let wire = message.encode().unwrap();
            assert!(wire.len() <= MAX_FRAME_SIZE);
            let Message::Chunk { id, offset, data } = Message::decode(&wire).unwrap() else {
                panic!()
            };
            let ack = receiver.write_chunk(&id, offset, &data).unwrap();
            assert_eq!(ack, sender.transferred());
            assert!(!destination.exists());
            count += 1;
        }
        assert_eq!(count, 4);
        assert_eq!(receiver.finish().unwrap(), destination);
        assert_eq!(fs::read(destination).unwrap(), bytes);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
    #[test]
    fn zero_byte_files_and_control_messages_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let offer = empty_offer();
        for message in [
            Message::Offer(offer.clone()),
            Message::Accept {
                id: offer.id.clone(),
            },
            Message::Reject {
                id: offer.id.clone(),
                reason: RejectReason::Declined,
            },
            Message::ChunkAck {
                id: offer.id.clone(),
                offset: 0,
            },
            Message::Finish {
                id: offer.id.clone(),
            },
            Message::Complete {
                id: offer.id.clone(),
            },
            Message::Cancel {
                id: offer.id.clone(),
            },
        ] {
            assert_eq!(
                Message::decode(&message.encode().unwrap()).unwrap(),
                message
            );
        }
        let destination = dir.path().join("empty.txt");
        Receiver::accept(offer, &destination)
            .unwrap()
            .finish()
            .unwrap();
        assert_eq!(fs::metadata(destination).unwrap().len(), 0);
    }
    #[test]
    fn corrupted_and_incomplete_content_is_removed() {
        for corruption in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let sender = Sender::open(source(dir.path(), b"original content")).unwrap();
            let destination = dir.path().join("received.txt");
            let mut receiver = Receiver::accept(sender.offer().clone(), &destination).unwrap();
            if corruption {
                receiver
                    .write_chunk(&sender.offer().id, 0, b"modified content")
                    .unwrap();
            }
            assert!(matches!(receiver.finish(), Err(Error::Integrity)));
            assert!(!destination.exists());
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
    #[test]
    fn cancellation_drops_partial_and_never_touches_destination() {
        let dir = tempfile::tempdir().unwrap();
        let sender = Sender::open(source(dir.path(), b"partial content")).unwrap();
        let destination = dir.path().join("received.txt");
        let mut receiver = Receiver::accept(sender.offer().clone(), &destination).unwrap();
        receiver
            .write_chunk(&sender.offer().id, 0, b"partial")
            .unwrap();
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        drop(receiver);
        assert!(!destination.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn existing_files_and_racing_file_creation_cannot_be_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("received.txt");
        fs::write(&destination, b"keep me").unwrap();
        assert!(matches!(
            Receiver::accept(empty_offer(), &destination),
            Err(Error::Exists)
        ));
        fs::remove_file(&destination).unwrap();
        let receiver = Receiver::accept(empty_offer(), &destination).unwrap();
        fs::write(&destination, b"created after acceptance").unwrap();
        assert!(matches!(receiver.finish(), Err(Error::Exists)));
        assert_eq!(fs::read(&destination).unwrap(), b"created after acceptance");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn traversal_reserved_and_control_names_are_rejected() {
        for name in [
            "",
            ".",
            "..",
            "../secret",
            "a/b.txt",
            "a\\b.txt",
            "/absolute",
            "C:\\secret",
            "name:stream",
            "nul",
            "NUL.txt",
            "COM1",
            "LPT9.log",
            "COM¹.txt",
            "tail.",
            "tail ",
            "a\n.txt",
            "a\0.txt",
        ] {
            assert!(!valid_name(name), "accepted {name:?}");
            let mut offer = empty_offer();
            offer.name = name.into();
            assert!(offer.validate().is_err());
        }
        for name in [
            "normal.txt",
            "报告.pdf",
            "file name",
            ".hidden",
            "COM10.txt",
        ] {
            assert!(valid_name(name));
        }
        assert!(!valid_name(&"a".repeat(256)));
    }
    #[test]
    fn malformed_metadata_and_chunks_are_bounded() {
        let mut oversized = MAGIC.to_vec();
        oversized.push(0);
        oversized.extend(vec![b' '; MAX_METADATA_SIZE + 1]);
        assert!(matches!(Message::decode(&oversized), Err(Error::Limit)));
        let mut unknown = b"FSF1\0".to_vec();
        unknown.extend_from_slice(
            format!(
                "{{\"type\":\"accept\",\"id\":\"{}\",\"path\":\"/tmp/file\"}}",
                empty_offer().id
            )
            .as_bytes(),
        );
        assert!(matches!(Message::decode(&unknown), Err(Error::Invalid)));
        for data in [vec![], vec![0; CHUNK_SIZE + 1]] {
            assert!(
                Message::Chunk {
                    id: empty_offer().id,
                    offset: 0,
                    data
                }
                .encode()
                .is_err()
            );
        }
        let mut wire = b"FSF1\x01".to_vec();
        wire.extend(vec![0; 24 + CHUNK_SIZE + 1]);
        assert!(matches!(Message::decode(&wire), Err(Error::Limit)));
        let mut offer = empty_offer();
        offer.size = MAX_FILE_SIZE + 1;
        assert!(matches!(offer.validate(), Err(Error::Limit)));
    }
    #[test]
    fn out_of_order_duplicated_wrong_id_and_excess_chunks_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let sender = Sender::open(source(dir.path(), b"ab")).unwrap();
        let id = &sender.offer().id;
        let mut receiver =
            Receiver::accept(sender.offer().clone(), dir.path().join("received.txt")).unwrap();
        assert!(matches!(
            receiver.write_chunk(id, 1, b"a"),
            Err(Error::Sequence)
        ));
        assert!(matches!(
            receiver.write_chunk(&"00".repeat(16), 0, b"a"),
            Err(Error::Sequence)
        ));
        receiver.write_chunk(id, 0, b"a").unwrap();
        assert!(matches!(
            receiver.write_chunk(id, 0, b"a"),
            Err(Error::Sequence)
        ));
        assert!(matches!(
            receiver.write_chunk(id, 1, b"bc"),
            Err(Error::Limit)
        ));
        receiver.write_chunk(id, 1, b"b").unwrap();
        receiver.finish().unwrap();
    }
    #[test]
    fn source_modification_before_send_cannot_be_completed() {
        let dir = tempfile::tempdir().unwrap();
        let path = source(dir.path(), b"original");
        let mut sender = Sender::open(&path).unwrap();
        fs::write(path, b"modified").unwrap();
        assert!(sender.next_chunk().unwrap().is_some());
        assert!(matches!(sender.next_chunk(), Err(Error::Integrity)));
    }
    #[test]
    fn retained_source_handle_ignores_replaced_source_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = source(dir.path(), b"original");
        let mut sender = Sender::open(&path).unwrap();
        fs::rename(&path, dir.path().join("renamed-original.txt")).unwrap();
        fs::write(path, b"replacement").unwrap();
        let Some(Message::Chunk { data, .. }) = sender.next_chunk().unwrap() else {
            panic!()
        };
        assert_eq!(data, b"original");
        assert!(sender.next_chunk().unwrap().is_none());
    }
    #[test]
    fn cancellation_stops_hashing_and_final_publication() {
        let dir = tempfile::tempdir().unwrap();
        let path = source(dir.path(), b"original");
        assert!(matches!(
            Sender::open_cancellable(path, &AtomicBool::new(true)),
            Err(Error::Cancelled)
        ));
        let destination = dir.path().join("received.txt");
        let receiver = Receiver::accept(empty_offer(), &destination).unwrap();
        assert!(matches!(
            receiver.finish_guarded::<()>(|| Err(Error::Cancelled)),
            Err(Error::Cancelled)
        ));
        assert!(!destination.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn cancellation_waiting_for_final_guard_never_publishes() {
        use std::sync::{Arc, Mutex, mpsc};
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("received.txt");
        let receiver = Receiver::accept(empty_offer(), &destination).unwrap();
        let lifecycle = Arc::new(Mutex::new(()));
        let cancelled = Arc::new(AtomicBool::new(false));
        let committed = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel();
        let (proceed_tx, proceed_rx) = mpsc::channel();
        let worker = std::thread::spawn({
            let lifecycle = lifecycle.clone();
            let cancelled = cancelled.clone();
            let committed = committed.clone();
            move || {
                receiver.finish_guarded_with(
                    || {
                        ready_tx.send(()).unwrap();
                        proceed_rx.recv().unwrap();
                        let guard = lifecycle.lock().unwrap();
                        if cancelled.load(Ordering::SeqCst) {
                            return Err(Error::Cancelled);
                        }
                        Ok(guard)
                    },
                    || committed.store(true, Ordering::SeqCst),
                )
            }
        });
        ready_rx.recv().unwrap();
        {
            let _guard = lifecycle.lock().unwrap();
            cancelled.store(true, Ordering::SeqCst);
        }
        proceed_tx.send(()).unwrap();
        assert!(matches!(worker.join().unwrap(), Err(Error::Cancelled)));
        assert!(!committed.load(Ordering::SeqCst));
        assert!(!destination.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }
    #[test]
    fn committed_callback_observes_publication_with_lifecycle_guard_held() {
        use std::sync::{Mutex, TryLockError};
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("received.txt");
        let receiver = Receiver::accept(empty_offer(), &destination).unwrap();
        let lifecycle = Mutex::new(());
        let published = AtomicBool::new(false);
        receiver
            .finish_guarded_with(
                || Ok(lifecycle.lock().unwrap()),
                || {
                    assert!(destination.exists());
                    assert!(matches!(
                        lifecycle.try_lock(),
                        Err(TryLockError::WouldBlock)
                    ));
                    published.store(true, Ordering::SeqCst);
                },
            )
            .unwrap();
        assert!(published.load(Ordering::SeqCst));
        assert!(lifecycle.try_lock().is_ok());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn replaced_parent_cannot_redirect_commit_or_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let selected = dir.path().join("selected");
        let moved = dir.path().join("moved");
        let outside = dir.path().join("outside");
        fs::create_dir(&selected).unwrap();
        fs::create_dir(&outside).unwrap();
        let destination = selected.join("received.txt");
        let receiver = Receiver::accept(empty_offer(), &destination).unwrap();
        fs::rename(&selected, &moved).unwrap();
        std::os::unix::fs::symlink(&outside, &selected).unwrap();
        receiver.finish().unwrap();
        assert!(!outside.join("received.txt").exists());
        assert!(moved.join("received.txt").exists());
        assert_eq!(fs::read_dir(&moved).unwrap().count(), 1);
        let second = Receiver::accept(empty_offer(), moved.join("cancelled.txt")).unwrap();
        let moved_again = dir.path().join("moved-again");
        fs::rename(&moved, &moved_again).unwrap();
        std::os::unix::fs::symlink(&outside, &moved).unwrap();
        drop(second);
        assert_eq!(fs::read_dir(&moved_again).unwrap().count(), 1);
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    }
    #[cfg(windows)]
    #[test]
    fn receiver_retains_non_deletable_parent_handle() {
        let dir = tempfile::tempdir().unwrap();
        let selected = dir.path().join("selected");
        fs::create_dir(&selected).unwrap();
        let receiver = Receiver::accept(empty_offer(), selected.join("received.txt")).unwrap();
        assert!(fs::rename(&selected, dir.path().join("moved")).is_err());
        drop(receiver);
        fs::rename(selected, dir.path().join("moved")).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn dangling_destination_symlink_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("received.txt");
        std::os::unix::fs::symlink(dir.path().join("missing"), &destination).unwrap();
        assert!(matches!(
            Receiver::accept(empty_offer(), destination),
            Err(Error::Exists)
        ));
    }
    #[cfg(unix)]
    #[test]
    fn fifo_and_directory_sources_are_rejected_without_opening() {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("fifo.txt");
        let fifo_name = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);
        assert!(matches!(Sender::open(&fifo), Err(Error::Invalid)));
        assert!(matches!(Sender::open(dir.path()), Err(Error::Invalid)));
    }
}
