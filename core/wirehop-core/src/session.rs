//! Blocking transfer sessions over a TCP stream, protocol v1.
//!
//! Implements the sequence in `docs/references/PROTOCOL.md` §"Transfer
//! session": raw key exchange, then length-prefixed encrypted frames carrying
//! metadata → response → file data → acknowledgment.
//!
//! Blocking I/O is a deliberate M0 choice. The core has no async runtime yet
//! and needs none to prove interoperability; the v2 transport milestone is
//! where a runtime becomes worth its cost.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::crypto::{Crypto, PUBLIC_KEY_LEN};
use crate::frame::{self, FrameReader, TRANSFER_QUANTUM};
use crate::message::{self, FileMetadata, Metadata};
use crate::protocol::{self, PeerNegotiation};
use crate::store::PartialFile;
use crate::Error;

/// Full acknowledgment window for a peer that negotiated `ack`.
pub const ACK_TIMEOUT: Duration = Duration::from_secs(10);
/// Short grace for a capless peer, which may still acknowledge or simply
/// close. See the `ack` capability notes in `PROTOCOL.md`.
pub const ACK_GRACE_TIMEOUT: Duration = Duration::from_secs(2);
/// Bound on how long any single blocking read may stall.
pub const IO_TIMEOUT: Duration = Duration::from_secs(30);

/// How a completed send ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendOutcome {
    /// Every byte was sent and the receiver acknowledged delivery.
    Confirmed,
    /// Every byte was sent, but no acknowledgment arrived. Not an error: a
    /// legacy peer closes without acknowledging.
    SentUnconfirmed,
    /// The receiving user declined.
    Rejected,
}

/// What a completed receive produced.
#[derive(Debug, Clone)]
pub struct ReceiveOutcome {
    pub device_name: String,
    pub session_code: String,
    pub peer: PeerNegotiation,
    pub files: Vec<PathBuf>,
    pub accepted: bool,
}

/// Reads exactly `len` bytes, or fails.
fn read_exact(stream: &mut TcpStream, len: usize) -> Result<Vec<u8>, Error> {
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).map_err(Error::Io)?;
    Ok(buf)
}

/// Pulls bytes until one whole frame is decoded.
fn next_frame(
    stream: &mut TcpStream,
    reader: &mut FrameReader,
    crypto: &Crypto,
) -> Result<Vec<u8>, Error> {
    loop {
        if let Some(frame) = reader.next_frame(crypto)? {
            return Ok(frame);
        }
        let mut chunk = [0u8; 16 * 1024];
        let read = stream.read(&mut chunk).map_err(Error::Io)?;
        if read == 0 {
            return Err(Error::Protocol("peer closed before completing a frame"));
        }
        reader.push(&chunk[..read]);
    }
}

/// Performs the key exchange and returns the established session.
fn handshake(stream: &mut TcpStream) -> Result<Crypto, Error> {
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(Error::Io)?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(Error::Io)?;

    let mut crypto = Crypto::new();
    // Both sides send immediately and unprompted; neither waits for the other,
    // so the exchange cannot deadlock.
    stream
        .write_all(&crypto.local_public_key())
        .map_err(Error::Io)?;
    let remote = read_exact(stream, PUBLIC_KEY_LEN)?;
    crypto.set_remote_public_key(&remote)?;
    Ok(crypto)
}

/// Sends `paths` to a connected peer.
pub fn send_files(
    stream: &mut TcpStream,
    device_name: &str,
    paths: &[PathBuf],
) -> Result<SendOutcome, Error> {
    let crypto = handshake(stream)?;
    let mut reader = FrameReader::new();

    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(Error::Protocol("file has no usable name"))?
            .to_string();
        let size = std::fs::metadata(path).map_err(Error::Io)?.len();
        files.push(FileMetadata { filename, size });
    }

    let metadata = Metadata {
        device_name: device_name.to_string(),
        device_type: std::env::consts::OS.to_string(),
        files: files.clone(),
    };
    stream
        .write_all(&frame::encode(&crypto, &metadata.to_canonical_json())?)
        .map_err(Error::Io)?;

    let (accepted, peer) = message::parse_response(&next_frame(stream, &mut reader, &crypto)?)?;
    if !accepted {
        return Ok(SendOutcome::Rejected);
    }

    // File data. Each frame is filled from exactly one file, and a zero-byte
    // file contributes no frames at all.
    for (path, meta) in paths.iter().zip(files.iter()) {
        let mut file = std::fs::File::open(path).map_err(Error::Io)?;
        let mut remaining = meta.size;
        let mut buf = vec![0u8; TRANSFER_QUANTUM];
        while remaining > 0 {
            let want = TRANSFER_QUANTUM.min(remaining as usize);
            let read = file.read(&mut buf[..want]).map_err(Error::Io)?;
            if read == 0 {
                return Err(Error::Protocol("file shrank during transfer"));
            }
            stream
                .write_all(&frame::encode(&crypto, &buf[..read])?)
                .map_err(Error::Io)?;
            remaining -= read as u64;
        }
    }

    // Only a peer that negotiated `ack` earns the full window.
    let window = if peer.has_negotiated_cap(protocol::CAP_ACK) {
        ACK_TIMEOUT
    } else {
        ACK_GRACE_TIMEOUT
    };
    stream.set_read_timeout(Some(window)).map_err(Error::Io)?;

    match next_frame(stream, &mut reader, &crypto) {
        Ok(data) if message::is_ack(&data) => Ok(SendOutcome::Confirmed),
        // A timeout or a close is qualified success, not failure: the bytes
        // were delivered to the socket and a legacy peer never acknowledges.
        _ => Ok(SendOutcome::SentUnconfirmed),
    }
}

/// A transfer that has been announced but not yet answered.
///
/// The handshake and metadata exchange are complete, so the session code and
/// file list can be shown to a user, and nothing has been written to disk.
/// The peer is waiting for the response frame.
///
/// This two-phase shape exists because a graphical shell cannot use a single
/// call that takes the accept decision as a parameter: it must show the code
/// and the file list, wait for a person, and only then answer. Holding the
/// connection open across that wait is the whole point — the peer's own
/// response timeout bounds how long a user has.
pub struct IncomingTransfer<'a> {
    stream: &'a mut TcpStream,
    crypto: Crypto,
    reader: FrameReader,
    metadata: Metadata,
    peer: PeerNegotiation,
    session_code: String,
}

impl<'a> IncomingTransfer<'a> {
    /// Performs the handshake and reads the metadata, stopping before the
    /// response so the caller can decide.
    pub fn receive_request(stream: &'a mut TcpStream) -> Result<Self, Error> {
        let crypto = handshake(stream)?;
        let session_code = crypto.session_key_digest()?;
        let mut reader = FrameReader::new();
        let (metadata, peer) = Metadata::parse(&next_frame(stream, &mut reader, &crypto)?)?;

        Ok(Self {
            stream,
            crypto,
            reader,
            metadata,
            peer,
            session_code,
        })
    }

    /// The six digits both users compare out of band.
    pub fn session_code(&self) -> &str {
        &self.session_code
    }

    /// The sender's self-reported name. Untrusted display text.
    pub fn device_name(&self) -> &str {
        &self.metadata.device_name
    }

    /// Files the sender declared, already validated against the policy bounds.
    pub fn files(&self) -> &[FileMetadata] {
        &self.metadata.files
    }

    pub fn total_size(&self) -> u64 {
        self.metadata.total_size()
    }

    pub fn peer(&self) -> &PeerNegotiation {
        &self.peer
    }

    fn respond(&mut self, accepted: bool) -> Result<(), Error> {
        let frame = frame::encode(&self.crypto, &message::response_to_canonical_json(accepted))?;
        self.stream.write_all(&frame).map_err(Error::Io)
    }

    /// Declines the transfer. Nothing is written to disk.
    pub fn reject(mut self) -> Result<ReceiveOutcome, Error> {
        self.respond(false)?;
        Ok(ReceiveOutcome {
            device_name: self.metadata.device_name.clone(),
            session_code: self.session_code.clone(),
            peer: self.peer.clone(),
            files: Vec::new(),
            accepted: false,
        })
    }

    /// Accepts and receives the files into `download_dir`.
    pub fn accept(mut self, download_dir: &Path) -> Result<ReceiveOutcome, Error> {
        self.respond(true)?;

        let mut outcome = ReceiveOutcome {
            device_name: self.metadata.device_name.clone(),
            session_code: self.session_code.clone(),
            peer: self.peer.clone(),
            files: Vec::new(),
            accepted: true,
        };

        // Frame boundaries and file boundaries are independent: one frame may
        // finish one file and begin the next, so data is consumed against the
        // declared remaining counts rather than per frame.
        let mut pending: Vec<u8> = Vec::new();
        for (index, meta) in self.metadata.files.iter().enumerate() {
            let mut partial = PartialFile::create(download_dir, index as u32)?;
            let mut remaining = meta.size;

            while remaining > 0 {
                if pending.is_empty() {
                    pending = next_frame(self.stream, &mut self.reader, &self.crypto)?;
                }
                let take = (remaining as usize).min(pending.len());
                partial.write_all(&pending[..take])?;
                pending.drain(..take);
                remaining -= take as u64;
            }
            outcome
                .files
                .push(partial.commit(download_dir, &meta.filename)?);
        }

        if !pending.is_empty() {
            return Err(Error::Protocol("received more file data than declared"));
        }

        // Best effort: a failure here must not fail a transfer already
        // committed to disk, and legacy senders ignore the frame entirely.
        if let Ok(ack) = frame::encode(&self.crypto, &message::ack_to_canonical_json()) {
            let _ = self.stream.write_all(&ack);
            let _ = self.stream.flush();
        }
        Ok(outcome)
    }
}

/// Receives a transfer into `download_dir`, deciding up front.
///
/// Convenience over [`IncomingTransfer`] for callers that already know the
/// answer — the CLI and the tests. A shell should use the two-phase form.
pub fn receive_files(
    stream: &mut TcpStream,
    download_dir: &Path,
    accept: bool,
) -> Result<ReceiveOutcome, Error> {
    let request = IncomingTransfer::receive_request(stream)?;
    if accept {
        request.accept(download_dir)
    } else {
        request.reject()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn tempdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "wirehop-session-{}-{}-{:?}",
            tag,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Runs a full session between two threads of this process.
    fn loopback(files: Vec<(&str, Vec<u8>)>, accept: bool) -> (SendOutcome, ReceiveOutcome) {
        let source = tempdir("src");
        let dest = tempdir("dst");

        let paths: Vec<PathBuf> = files
            .iter()
            .map(|(name, content)| {
                let path = source.join(name);
                std::fs::write(&path, content).unwrap();
                path
            })
            .collect();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let dest_for_thread = dest.clone();
        let receiver = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            receive_files(&mut stream, &dest_for_thread, accept).unwrap()
        });

        let mut client = TcpStream::connect(addr).unwrap();
        let sent = send_files(&mut client, "rust-peer", &paths).unwrap();
        let received = receiver.join().unwrap();
        (sent, received)
    }

    #[test]
    fn delivers_files_byte_for_byte() {
        let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let (sent, received) = loopback(
            vec![
                ("a.txt", b"hello loopback".to_vec()),
                ("b.bin", big.clone()),
                ("empty.dat", Vec::new()),
            ],
            true,
        );

        assert_eq!(sent, SendOutcome::Confirmed);
        assert_eq!(received.device_name, "rust-peer");
        assert_eq!(received.files.len(), 3);
        assert_eq!(
            std::fs::read(&received.files[0]).unwrap(),
            b"hello loopback"
        );
        assert_eq!(std::fs::read(&received.files[1]).unwrap(), big);
        assert_eq!(std::fs::read(&received.files[2]).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn spans_file_boundaries_within_the_declared_counts() {
        // Two files whose sizes are not frame-aligned, so the receiver must
        // carry leftover bytes from one file's frame into the next file.
        let first: Vec<u8> = vec![b'x'; TRANSFER_QUANTUM + 500];
        let second: Vec<u8> = vec![b'y'; 1_000];
        let (sent, received) = loopback(
            vec![("first.bin", first.clone()), ("second.bin", second.clone())],
            true,
        );

        assert_eq!(sent, SendOutcome::Confirmed);
        assert_eq!(std::fs::read(&received.files[0]).unwrap(), first);
        assert_eq!(std::fs::read(&received.files[1]).unwrap(), second);
    }

    #[test]
    fn negotiates_capabilities_in_both_directions() {
        let (_, received) = loopback(vec![("a.txt", b"caps".to_vec())], true);
        assert_eq!(received.peer.version, protocol::VERSION);
        assert!(received.peer.has_negotiated_cap(protocol::CAP_ACK));
        assert_eq!(received.session_code.len(), 6);
    }

    /// The property a graphical shell depends on: everything needed to ask a
    /// user is available *before* answering, and nothing has touched disk yet.
    #[test]
    fn a_request_can_be_inspected_before_it_is_answered() {
        let source = tempdir("two-phase-src");
        let dest = tempdir("two-phase-dst");
        let content = b"decide first".to_vec();
        let path = source.join("a.txt");
        std::fs::write(&path, &content).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let dest_for_thread = dest.clone();

        let receiver = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = IncomingTransfer::receive_request(&mut stream).unwrap();

            // Everything a prompt needs, before responding.
            let code = request.session_code().to_string();
            assert_eq!(code.len(), 6);
            assert_eq!(request.device_name(), "rust-peer");
            assert_eq!(request.files().len(), 1);
            assert_eq!(request.files()[0].filename, "a.txt");
            assert_eq!(request.total_size(), 12);
            assert!(request.peer().has_negotiated_cap(protocol::CAP_ACK));

            // Nothing on disk yet: the user has not decided.
            assert!(std::fs::read_dir(&dest_for_thread)
                .unwrap()
                .next()
                .is_none());

            // Stand in for a person taking a moment to compare the code.
            std::thread::sleep(Duration::from_millis(150));
            let outcome = request.accept(&dest_for_thread).unwrap();
            (code, outcome)
        });

        let mut client = TcpStream::connect(addr).unwrap();
        let sent = send_files(&mut client, "rust-peer", &[path]).unwrap();
        let (code, outcome) = receiver.join().unwrap();

        assert_eq!(sent, SendOutcome::Confirmed);
        assert_eq!(outcome.session_code, code);
        assert_eq!(std::fs::read(&outcome.files[0]).unwrap(), content);
    }

    #[test]
    fn a_rejected_request_reports_the_code_and_writes_nothing() {
        let source = tempdir("reject-src");
        let dest = tempdir("reject-dst");
        let path = source.join("a.txt");
        std::fs::write(&path, b"nope").unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let receiver = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = IncomingTransfer::receive_request(&mut stream).unwrap();
            assert_eq!(request.session_code().len(), 6);
            request.reject().unwrap()
        });

        let mut client = TcpStream::connect(addr).unwrap();
        let sent = send_files(&mut client, "rust-peer", &[path]).unwrap();
        let outcome = receiver.join().unwrap();

        assert_eq!(sent, SendOutcome::Rejected);
        assert!(!outcome.accepted);
        assert!(outcome.files.is_empty());
        assert!(std::fs::read_dir(&dest).unwrap().next().is_none());
    }

    #[test]
    fn rejection_leaves_no_files_behind() {
        let (sent, received) = loopback(vec![("a.txt", b"nope".to_vec())], false);
        assert_eq!(sent, SendOutcome::Rejected);
        assert!(received.files.is_empty());
    }
}
