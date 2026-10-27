//! Wire protocol. Every request opens a fresh bidirectional QUIC stream:
//! the client writes one length-prefixed JSON request and finishes its side,
//! the server answers with one or more length-prefixed JSON frames. A chunk
//! response is followed by the chunk's bytes in 1 MiB blocks, each block
//! followed by its 32-byte BLAKE3 hash, so the receiver can verify and save
//! progress block by block and resume a chunk part-way through.

use anyhow::{bail, Context, Result};
use iroh::endpoint::{RecvStream, SendStream};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub const ALPN: &[u8] = b"relayless/1";
pub const PROTOCOL_VERSION: u32 = 2;
/// Unit of verification and resumable progress inside a chunk.
pub const BLOCK: u64 = 1024 * 1024;
/// Largest JSON frame we accept (directory listings are paginated well below this).
pub const MAX_FRAME: usize = 4 * 1024 * 1024;
/// Largest chunk a peer may request.
pub const MAX_CHUNK: u64 = 64 * 1024 * 1024;
/// Largest page of a directory listing.
pub const MAX_LIST_PAGE: u32 = 1000;
/// Largest recursive walk (number of files) for a folder download.
pub const MAX_WALK_FILES: usize = 50_000;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Request {
    Hello {
        v: u32,
        token: Option<String>,
        device_name: String,
    },
    List {
        share: String,
        path: Vec<String>,
        offset: u32,
        limit: u32,
    },
    Walk {
        share: String,
        path: Vec<String>,
    },
    Stat {
        share: String,
        path: Vec<String>,
    },
    Chunk {
        share: String,
        path: Vec<String>,
        size: u64,
        mtime_ms: i64,
        chunk_size: u64,
        index: u64,
        /// Byte offset inside the chunk to start from (a multiple of BLOCK).
        #[serde(default)]
        from: u64,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HelloStatus {
    Approved,
    Pending,
    Denied,
    InvalidInvite,
    Expired,
    Revoked,
    UsedUp,
    Unsupported,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WireShare {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HelloResp {
    pub status: HelloStatus,
    pub sender_name: String,
    pub shares: Vec<WireShare>,
    pub message: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WireEntry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub mtime_ms: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WalkFile {
    pub path: Vec<String>,
    pub size: u64,
    pub mtime_ms: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrCode {
    Denied,
    NotFound,
    SourceChanged,
    RelayBlocked,
    BadRequest,
    Io,
    InUse,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "r", rename_all = "snake_case")]
pub enum Response {
    Hello(HelloResp),
    List { entries: Vec<WireEntry>, total: u64 },
    Walk { files: Vec<WalkFile>, truncated: bool },
    Stat { size: u64, mtime_ms: i64, dir: bool },
    Chunk { len: u64 },
    Error { code: ErrCode, message: String },
}

impl Response {
    pub fn err(code: ErrCode, message: impl Into<String>) -> Self {
        Response::Error { code, message: message.into() }
    }
}

/// Error surfaced to the receiver when the sender rejects a request.
#[derive(Debug, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct RemoteError {
    pub code: ErrCode,
    pub message: String,
}

pub async fn write_frame<T: Serialize>(send: &mut SendStream, value: &T) -> Result<()> {
    let buf = serde_json::to_vec(value)?;
    if buf.len() > MAX_FRAME {
        bail!("frame too large");
    }
    send.write_all(&(buf.len() as u32).to_be_bytes()).await?;
    send.write_all(&buf).await?;
    Ok(())
}

pub async fn read_frame<T: DeserializeOwned>(recv: &mut RecvStream) -> Result<T> {
    let mut len = [0u8; 4];
    recv.read_exact(&mut len).await.context("reading frame length")?;
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_FRAME {
        bail!("peer sent oversized frame ({len} bytes)");
    }
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf).await.context("reading frame body")?;
    Ok(serde_json::from_slice(&buf).context("decoding frame")?)
}

/// Hash recorded for a finished chunk: BLAKE3 over its block hashes.
pub fn chunk_hash_from_blocks(blocks: &[[u8; 32]]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"relayless-chunk-v2");
    for b in blocks {
        h.update(b);
    }
    *h.finalize().as_bytes()
}

/// Validates path components received from the network: no traversal,
/// no separators, no drive prefixes, no NUL, bounded length.
pub fn validate_components(path: &[String]) -> Result<()> {
    if path.len() > 128 {
        bail!("path too deep");
    }
    for c in path {
        if c.is_empty()
            || c == "."
            || c == ".."
            || c.len() > 255
            || c.contains(['/', '\\', ':', '\0'])
        {
            bail!("invalid path component");
        }
    }
    Ok(())
}
