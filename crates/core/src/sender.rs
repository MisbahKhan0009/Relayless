//! Sender side: accepts connections, authorizes every request against the
//! share grants stored locally, and streams chunks from disk.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use iroh::endpoint::{Connection, RecvStream, SendStream, VarInt};
use iroh::EndpointId;
use tokio::sync::oneshot;
use tracing::{debug, warn};

use crate::db::ShareRow;
use crate::inner::{Inner, PendingAccess};
use crate::invite::token_hash;
use crate::net::route_of;
use crate::proto::*;
use crate::types::{AccessRequest, CoreEvent};
use crate::util::{
    now_ms, open_source, random_id, read_exact_at, short_peer, system_time_ms,
};

const BLOCK: usize = 1024 * 1024;
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub async fn accept_loop(inner: Arc<Inner>) {
    loop {
        let incoming = tokio::select! {
            i = inner.endpoint.accept() => i,
            _ = inner.shutdown.cancelled() => break,
        };
        let Some(incoming) = incoming else { break };
        let inner = inner.clone();
        tokio::spawn(async move {
            let accepting = match incoming.accept() {
                Ok(a) => a,
                Err(e) => {
                    debug!("incoming rejected: {e:#}");
                    return;
                }
            };
            match accepting.await {
                Ok(conn) => handle_conn(inner, conn).await,
                Err(e) => debug!("handshake failed: {e:#}"),
            }
        });
    }
}

async fn handle_conn(inner: Arc<Inner>, conn: Connection) {
    let peer = conn.remote_id();
    let conn_id = random_id();
    inner.incoming.lock().entry(peer).or_default().push((conn_id.clone(), conn.clone()));
    inner.emit(CoreEvent::GrantsChanged);

    // Report route changes for the sharing view.
    let watcher = {
        let inner = inner.clone();
        let conn = conn.clone();
        tokio::spawn(async move {
            use n0_future::StreamExt;
            let mut ev = conn.path_events();
            while ev.next().await.is_some() {
                inner.emit(CoreEvent::GrantsChanged);
            }
        })
    };

    loop {
        let (send, recv) = match conn.accept_bi().await {
            Ok(s) => s,
            Err(_) => break,
        };
        let inner = inner.clone();
        let conn = conn.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_stream(&inner, peer, &conn, send, recv).await {
                debug!("request from {} failed: {e:#}", peer.fmt_short());
            }
        });
    }

    watcher.abort();
    {
        let mut map = inner.incoming.lock();
        if let Some(v) = map.get_mut(&peer) {
            v.retain(|(id, _)| id != &conn_id);
            if v.is_empty() {
                map.remove(&peer);
            }
        }
    }
    inner.emit(CoreEvent::GrantsChanged);
}

async fn handle_stream(
    inner: &Arc<Inner>,
    peer: EndpointId,
    conn: &Connection,
    mut send: SendStream,
    mut recv: RecvStream,
) -> Result<()> {
    let req: Request = tokio::time::timeout(Duration::from_secs(30), read_frame(&mut recv)).await??;
    match req {
        Request::Hello { v, token, device_name } => {
            handle_hello(inner, peer, conn, &mut send, v, token, device_name).await?;
        }
        Request::List { share, path, offset, limit } => {
            let resp = match authorize(inner, peer, &share) {
                Ok(s) => list_dir(&s, &path, offset, limit.min(MAX_LIST_PAGE)).await,
                Err(r) => r,
            };
            write_frame(&mut send, &resp).await?;
        }
        Request::Walk { share, path } => {
            let resp = match authorize(inner, peer, &share) {
                Ok(s) => walk_dir(&s, &path).await,
                Err(r) => r,
            };
            write_frame(&mut send, &resp).await?;
        }
        Request::Stat { share, path } => {
            let resp = match authorize(inner, peer, &share) {
                Ok(s) => stat(&s, &path).await,
                Err(r) => r,
            };
            write_frame(&mut send, &resp).await?;
        }
        Request::Chunk { share, path, size, mtime_ms, chunk_size, index } => {
            match authorize(inner, peer, &share) {
                Ok(s) => {
                    serve_chunk(inner, peer, conn, &mut send, &s, &path, size, mtime_ms, chunk_size, index)
                        .await?
                }
                Err(r) => write_frame(&mut send, &r).await?,
            }
        }
    }
    let _ = send.finish();
    let _ = send.stopped().await;
    Ok(())
}

// ------------------------------------------------------------------ authorization

fn authorize(inner: &Inner, peer: EndpointId, share_id: &str) -> Result<ShareRow, Response> {
    let share = match inner.db.get_share(share_id) {
        Ok(Some(s)) if s.active => s,
        _ => return Err(Response::err(ErrCode::Denied, "This folder is no longer shared.")),
    };
    let peer_s = peer.to_string();
    let granted = inner
        .db
        .list_grants()
        .map(|g| g.iter().any(|g| g.peer_id == peer_s && g.share_id == share_id))
        .unwrap_or(false);
    if !granted {
        return Err(Response::err(ErrCode::Denied, "The sender has not given this device access."));
    }
    Ok(share)
}

fn granted_shares(inner: &Inner, peer: EndpointId) -> Vec<WireShare> {
    let peer_s = peer.to_string();
    let grants = inner.db.list_grants().unwrap_or_default();
    let shares = inner.db.list_shares().unwrap_or_default();
    shares
        .into_iter()
        .filter(|s| s.active && grants.iter().any(|g| g.peer_id == peer_s && g.share_id == s.id))
        .map(|s| WireShare { id: s.id, name: s.name })
        .collect()
}

fn rate_limited(inner: &Inner) -> bool {
    let mut q = inner.failed_hellos.lock();
    let now = Instant::now();
    while q.front().is_some_and(|t: &Instant| now.duration_since(*t) > Duration::from_secs(60)) {
        q.pop_front();
    }
    q.len() >= 20
}

fn record_failure(inner: &Inner) {
    let mut q: parking_lot::MutexGuard<VecDeque<Instant>> = inner.failed_hellos.lock();
    q.push_back(Instant::now());
}

#[allow(clippy::too_many_arguments)]
async fn handle_hello(
    inner: &Arc<Inner>,
    peer: EndpointId,
    conn: &Connection,
    send: &mut SendStream,
    v: u32,
    token: Option<String>,
    device_name: String,
) -> Result<()> {
    let sender_name = inner.settings.read().device_name.clone();
    let device_name: String = device_name.chars().filter(|c| !c.is_control()).take(64).collect();
    let device_name = if device_name.trim().is_empty() { "Unnamed device".to_string() } else { device_name };
    let reply = |status: HelloStatus, shares: Vec<WireShare>, message: Option<&str>| {
        Response::Hello(HelloResp {
            status,
            sender_name: sender_name.clone(),
            shares,
            message: message.map(|s| s.to_string()),
        })
    };

    if v != PROTOCOL_VERSION {
        write_frame(send, &reply(HelloStatus::Unsupported, vec![], Some("Version mismatch. Update Relayless on both computers."))).await?;
        return Ok(());
    }
    let peer_s = peer.to_string();

    let Some(token) = token else {
        let shares = granted_shares(inner, peer);
        if shares.is_empty() {
            write_frame(send, &reply(HelloStatus::Denied, vec![], Some("The sender has removed this device's access."))).await?;
        } else {
            let _ = inner.db.touch_grants(&peer_s);
            write_frame(send, &reply(HelloStatus::Approved, shares, None)).await?;
        }
        return Ok(());
    };

    if rate_limited(inner) {
        write_frame(send, &reply(HelloStatus::Denied, vec![], Some("Too many attempts. Try again in a minute."))).await?;
        return Ok(());
    }
    let inv = match inner.db.find_invitation_by_hash(&token_hash(&token))? {
        Some(i) => i,
        None => {
            record_failure(inner);
            write_frame(send, &reply(HelloStatus::InvalidInvite, vec![], Some("This invitation isn't valid on the sender's computer."))).await?;
            conn.close(VarInt::from_u32(401), b"invalid invitation");
            return Ok(());
        }
    };
    let share = match inner.db.get_share(&inv.share_id)? {
        Some(s) if s.active => s,
        _ => {
            write_frame(send, &reply(HelloStatus::Denied, vec![], Some("The shared folder is no longer available."))).await?;
            return Ok(());
        }
    };
    let already = inner
        .db
        .list_grants()?
        .iter()
        .any(|g| g.peer_id == peer_s && g.share_id == share.id);
    if already {
        let _ = inner.db.touch_grants(&peer_s);
        write_frame(send, &reply(HelloStatus::Approved, granted_shares(inner, peer), None)).await?;
        return Ok(());
    }
    match inv.state(now_ms()) {
        "revoked" => {
            write_frame(send, &reply(HelloStatus::Revoked, vec![], Some("The sender revoked this invitation."))).await?;
            return Ok(());
        }
        "expired" => {
            write_frame(send, &reply(HelloStatus::Expired, vec![], Some("This invitation has expired. Ask the sender for a new one."))).await?;
            return Ok(());
        }
        "used_up" => {
            write_frame(send, &reply(HelloStatus::UsedUp, vec![], Some("This invitation has already been used. Ask the sender for a new one."))).await?;
            return Ok(());
        }
        _ => {}
    }

    if inv.require_approval {
        write_frame(send, &reply(HelloStatus::Pending, vec![], Some("Waiting for the sender to approve this device."))).await?;
        let (tx, rx) = oneshot::channel();
        let req = AccessRequest {
            id: random_id(),
            peer_id: peer_s.clone(),
            peer_short: short_peer(&peer_s),
            device_name: device_name.clone(),
            share_id: share.id.clone(),
            share_name: share.name.clone(),
            created_at: now_ms(),
        };
        inner.pending.lock().insert(req.id.clone(), PendingAccess { request: req.clone(), respond: tx });
        inner.emit(CoreEvent::AccessRequested(req.clone()));
        inner.notify(
            "New device wants access",
            &format!("{} is asking to open \"{}\". Open Relayless to approve.", device_name, share.name),
        );
        let decision = tokio::select! {
            r = rx => r.unwrap_or(false),
            _ = tokio::time::sleep(APPROVAL_TIMEOUT) => false,
            _ = conn.closed() => false,
        };
        inner.pending.lock().remove(&req.id);
        inner.emit(CoreEvent::AccessResolved(req.id.clone()));
        if !decision {
            inner.log("warning", &format!("Access for {} to \"{}\" was not approved", device_name, share.name), None);
            write_frame(send, &reply(HelloStatus::Denied, vec![], Some("The sender didn't approve this device."))).await?;
            return Ok(());
        }
    }
    inner.db.upsert_grant(&peer_s, &share.id, &device_name)?;
    inner.db.increment_invitation_use(&inv.id)?;
    inner.log("success", &format!("{} can now open \"{}\"", device_name, share.name), None);
    inner.emit(CoreEvent::GrantsChanged);
    inner.emit(CoreEvent::SharesChanged);
    write_frame(send, &reply(HelloStatus::Approved, granted_shares(inner, peer), None)).await?;
    Ok(())
}

// ------------------------------------------------------------------ filesystem access

/// Resolves remote path components strictly inside the share root. Symlinks
/// and junctions are resolved by canonicalization and rejected if they escape.
pub fn resolve_in_share(root: &Path, comps: &[String]) -> Result<PathBuf, Response> {
    validate_components(comps).map_err(|_| Response::err(ErrCode::BadRequest, "Invalid path."))?;
    let root_c = std::fs::canonicalize(root)
        .map_err(|_| Response::err(ErrCode::NotFound, "The shared folder is missing on the sender's computer."))?;
    let mut p = root_c.clone();
    for c in comps {
        p.push(c);
    }
    let real = std::fs::canonicalize(&p).map_err(|_| Response::err(ErrCode::NotFound, "Not found."))?;
    if !real.starts_with(&root_c) {
        return Err(Response::err(ErrCode::Denied, "Outside the shared folder."));
    }
    Ok(real)
}

async fn list_dir(share: &ShareRow, path: &[String], offset: u32, limit: u32) -> Response {
    let root = PathBuf::from(&share.root_path);
    let path = path.to_vec();
    tokio::task::spawn_blocking(move || {
        let dir = match resolve_in_share(&root, &path) {
            Ok(d) => d,
            Err(r) => return r,
        };
        let rd = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) => return Response::err(ErrCode::Io, format!("Can't open folder: {e}")),
        };
        let mut entries = Vec::new();
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            let Ok(meta) = e.metadata() else { continue };
            let name = e.file_name().to_string_lossy().to_string();
            entries.push(WireEntry {
                name,
                dir: ft.is_dir(),
                size: if ft.is_dir() { 0 } else { meta.len() },
                mtime_ms: system_time_ms(meta.modified()),
            });
        }
        entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        let total = entries.len() as u64;
        let page = entries.into_iter().skip(offset as usize).take(limit.max(1) as usize).collect();
        Response::List { entries: page, total }
    })
    .await
    .unwrap_or_else(|_| Response::err(ErrCode::Io, "internal error"))
}

async fn walk_dir(share: &ShareRow, path: &[String]) -> Response {
    let root = PathBuf::from(&share.root_path);
    let path = path.to_vec();
    tokio::task::spawn_blocking(move || {
        let base = match resolve_in_share(&root, &path) {
            Ok(d) => d,
            Err(r) => return r,
        };
        let mut files = Vec::new();
        let mut stack: Vec<(PathBuf, Vec<String>)> = vec![(base, vec![])];
        let mut truncated = false;
        while let Some((dir, rel)) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_symlink() {
                    continue;
                }
                let name = e.file_name().to_string_lossy().to_string();
                let mut r = rel.clone();
                r.push(name);
                if ft.is_dir() {
                    stack.push((e.path(), r));
                } else if ft.is_file() {
                    let Ok(meta) = e.metadata() else { continue };
                    files.push(WalkFile { path: r, size: meta.len(), mtime_ms: system_time_ms(meta.modified()) });
                    if files.len() >= MAX_WALK_FILES {
                        truncated = true;
                        break;
                    }
                }
            }
            if truncated {
                break;
            }
        }
        Response::Walk { files, truncated }
    })
    .await
    .unwrap_or_else(|_| Response::err(ErrCode::Io, "internal error"))
}

async fn stat(share: &ShareRow, path: &[String]) -> Response {
    let root = PathBuf::from(&share.root_path);
    let path = path.to_vec();
    tokio::task::spawn_blocking(move || {
        let p = match resolve_in_share(&root, &path) {
            Ok(p) => p,
            Err(r) => return r,
        };
        match std::fs::metadata(&p) {
            Ok(m) => Response::Stat { size: m.len(), mtime_ms: system_time_ms(m.modified()), dir: m.is_dir() },
            Err(_) => Response::err(ErrCode::NotFound, "Not found."),
        }
    })
    .await
    .unwrap_or_else(|_| Response::err(ErrCode::Io, "internal error"))
}

#[allow(clippy::too_many_arguments)]
async fn serve_chunk(
    inner: &Arc<Inner>,
    peer: EndpointId,
    conn: &Connection,
    send: &mut SendStream,
    share: &ShareRow,
    path: &[String],
    size: u64,
    mtime_ms: i64,
    chunk_size: u64,
    index: u64,
) -> Result<()> {
    if !(64 * 1024..=MAX_CHUNK).contains(&chunk_size) || index.saturating_mul(chunk_size) >= size.max(1) {
        write_frame(send, &Response::err(ErrCode::BadRequest, "Invalid chunk request.")).await?;
        return Ok(());
    }
    if inner.settings.read().direct_only() && !route_of(conn).is_direct() {
        write_frame(send, &Response::err(ErrCode::RelayBlocked, "The sender only allows direct connections.")).await?;
        return Ok(());
    }
    let root = PathBuf::from(&share.root_path);
    let comps = path.to_vec();
    let opened = tokio::task::spawn_blocking(move || -> Result<std::fs::File, Response> {
        let p = resolve_in_share(&root, &comps)?;
        let f = open_source(&p).map_err(|e| {
            if e.raw_os_error() == Some(32) {
                Response::err(ErrCode::InUse, "The file is locked by another program on the sender's computer.")
            } else {
                Response::err(ErrCode::NotFound, "The file is no longer available.")
            }
        })?;
        let m = f.metadata().map_err(|_| Response::err(ErrCode::Io, "Can't read file."))?;
        if !m.is_file() || m.len() != size || system_time_ms(m.modified()) != mtime_ms {
            return Err(Response::err(ErrCode::SourceChanged, "The file changed on the sender's computer."));
        }
        Ok(f)
    })
    .await?;
    let file = match opened {
        Ok(f) => Arc::new(f),
        Err(resp) => {
            write_frame(send, &resp).await?;
            return Ok(());
        }
    };

    let offset = index * chunk_size;
    let len = chunk_size.min(size - offset);
    write_frame(send, &Response::Chunk { len }).await?;

    let display: String = path.last().cloned().unwrap_or_default();
    let stats = inner.upload_stats_for(peer);
    stats.begin(&display);

    let result: Result<()> = async {
        let mut hasher = blake3::Hasher::new();
        let mut buf = vec![0u8; BLOCK];
        let mut pos = offset;
        let mut remaining = len;
        let mut blocks = 0u32;
        while remaining > 0 {
            let n = (remaining as usize).min(BLOCK);
            inner.upload_limiter.acquire(n).await;
            let f = file.clone();
            let (b, h, r) = tokio::task::spawn_blocking(move || {
                let mut b = buf;
                let mut h = hasher;
                let r = read_exact_at(&f, &mut b[..n], pos);
                if r.is_ok() {
                    h.update(&b[..n]);
                }
                (b, h, r)
            })
            .await?;
            buf = b;
            hasher = h;
            if let Err(e) = r {
                warn!("read failed while serving chunk: {e}");
                send.reset(VarInt::from_u32(2)).ok();
                anyhow::bail!("read failed");
            }
            send.write_all(&buf[..n]).await?;
            stats.sent.fetch_add(n as u64, Ordering::Relaxed);
            pos += n as u64;
            remaining -= n as u64;
            blocks += 1;
            if blocks % 8 == 0 && inner.settings.read().direct_only() && !route_of(conn).is_direct() {
                send.reset(VarInt::from_u32(3)).ok();
                anyhow::bail!("route switched to relay");
            }
        }
        // Detect modifications that happened while this chunk was read.
        let f = file.clone();
        let unchanged = tokio::task::spawn_blocking(move || {
            f.metadata()
                .map(|m| m.len() == size && system_time_ms(m.modified()) == mtime_ms)
                .unwrap_or(false)
        })
        .await?;
        if !unchanged {
            send.reset(VarInt::from_u32(4)).ok();
            anyhow::bail!("source changed during read");
        }
        send.write_all(hasher.finalize().as_bytes()).await?;
        Ok(())
    }
    .await;
    stats.end(&display);
    result
}
