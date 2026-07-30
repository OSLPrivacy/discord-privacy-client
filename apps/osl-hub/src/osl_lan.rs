//! Direct encrypted LAN rooms for OSL Notes. No relay, discovery server, or cloud account.

use crate::osl_collab::{
    create_invitation, open_frame, parse_invitation, seal_frame, OslCollaborationFrame,
    OslLanInvitation,
};
use crate::osl_notes::OslDocumentKind;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use zeroize::Zeroizing;

const MAX_WIRE_BYTES: usize = 1_100_000;
const IO_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OslSharedDocument {
    pub kind: OslDocumentKind,
    pub title: String,
    pub body: String,
    pub folder: String,
    pub tags: Vec<String>,
    pub favorite: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslLanSession {
    pub session_id: String,
    pub role: &'static str,
    pub revision: u64,
    pub document: OslSharedDocument,
    pub invitation: Option<OslLanInvitation>,
    pub connected: bool,
    pub encrypted: bool,
    pub cloud: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslLanSync {
    pub revision: u64,
    pub document: OslSharedDocument,
    pub changed: bool,
    pub conflict: bool,
    pub connected: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", tag = "action", deny_unknown_fields)]
enum Request {
    Pull {
        known_revision: u64,
    },
    Push {
        base_revision: u64,
        document: OslSharedDocument,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", tag = "status", deny_unknown_fields)]
enum Response {
    Snapshot {
        revision: u64,
        document: OslSharedDocument,
        conflict: bool,
    },
    Unchanged {
        revision: u64,
    },
}

struct RoomState {
    revision: u64,
    document: OslSharedDocument,
}
struct HostedRoom {
    owner: String,
    state: Arc<Mutex<RoomState>>,
    stop: Arc<AtomicBool>,
}
struct JoinedRoom {
    owner: String,
    room_id: String,
    secret: Zeroizing<[u8; 32]>,
    stream: TcpStream,
    request_sequence: u64,
    response_sequence: u64,
    revision: u64,
    document: OslSharedDocument,
}

static HOSTED: OnceLock<Mutex<HashMap<String, HostedRoom>>> = OnceLock::new();
static JOINED: OnceLock<Mutex<HashMap<String, Arc<Mutex<JoinedRoom>>>>> = OnceLock::new();
fn hosted() -> &'static Mutex<HashMap<String, HostedRoom>> {
    HOSTED.get_or_init(|| Mutex::new(HashMap::new()))
}
fn joined() -> &'static Mutex<HashMap<String, Arc<Mutex<JoinedRoom>>>> {
    JOINED.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn host(owner: &str, document: OslSharedDocument) -> Result<OslLanSession, String> {
    validate_document(&document)?;
    let listener = TcpListener::bind("0.0.0.0:0")
        .map_err(|_| "OSL could not open a LAN collaboration port".to_owned())?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "OSL could not configure the LAN room".to_owned())?;
    let port = listener
        .local_addr()
        .map_err(|_| "The LAN room address is unavailable".to_owned())?
        .port();
    let address = SocketAddr::new(local_ipv4()?, port);
    let (invitation, secret) = create_invitation(address)?;
    let room_id = invitation.room_id.clone();
    let state = Arc::new(Mutex::new(RoomState {
        revision: 0,
        document: document.clone(),
    }));
    let stop = Arc::new(AtomicBool::new(false));
    hosted()
        .lock()
        .map_err(|_| "The LAN room registry is unavailable".to_owned())?
        .insert(
            room_id.clone(),
            HostedRoom {
                owner: owner.to_owned(),
                state: state.clone(),
                stop: stop.clone(),
            },
        );
    let thread_secret = Zeroizing::new(secret);
    if std::thread::Builder::new()
        .name(format!("osl-lan-{room_id}"))
        .spawn(move || serve(listener, room_id, thread_secret, state, stop))
        .is_err()
    {
        let _ = hosted()
            .lock()
            .map(|mut rooms| rooms.remove(&invitation.room_id));
        return Err("The LAN collaboration listener could not start".into());
    }
    Ok(OslLanSession {
        session_id: invitation.room_id.clone(),
        role: "host",
        revision: 0,
        document,
        invitation: Some(invitation),
        connected: true,
        encrypted: true,
        cloud: false,
    })
}

pub fn join(owner: &str, code: &str) -> Result<OslLanSession, String> {
    let (address, room_id, secret) = parse_invitation(code)?;
    let mut stream = TcpStream::connect_timeout(&address, IO_TIMEOUT)
        .map_err(|_| "That LAN room could not be reached on this network".to_owned())?;
    configure_stream(&stream)?;
    let response: Response = receive(&mut stream, &room_id, &secret, 0)?;
    let Response::Snapshot {
        revision, document, ..
    } = response
    else {
        return Err("The LAN host did not send an initial document".into());
    };
    validate_document(&document)?;
    let session_id = random_id(16);
    let connection = JoinedRoom {
        owner: owner.to_owned(),
        room_id,
        secret: Zeroizing::new(secret),
        stream,
        request_sequence: 0,
        response_sequence: 1,
        revision,
        document: document.clone(),
    };
    joined()
        .lock()
        .map_err(|_| "The LAN session registry is unavailable".to_owned())?
        .insert(session_id.clone(), Arc::new(Mutex::new(connection)));
    Ok(OslLanSession {
        session_id,
        role: "guest",
        revision,
        document,
        invitation: None,
        connected: true,
        encrypted: true,
        cloud: false,
    })
}

pub fn sync_host(
    owner: &str,
    room_id: &str,
    base_revision: u64,
    document: Option<OslSharedDocument>,
) -> Result<OslLanSync, String> {
    let rooms = hosted()
        .lock()
        .map_err(|_| "The LAN room registry is unavailable".to_owned())?;
    let room = rooms
        .get(room_id)
        .ok_or_else(|| "That LAN room is no longer open".to_owned())?;
    if room.owner != owner {
        return Err("That LAN room belongs to another OSL identity".into());
    }
    let mut state = room
        .state
        .lock()
        .map_err(|_| "The LAN room state is unavailable".to_owned())?;
    let mut conflict = false;
    if let Some(document) = document {
        validate_document(&document)?;
        if base_revision == state.revision {
            state.revision = state.revision.saturating_add(1);
            state.document = document;
        } else {
            conflict = true;
        }
    }
    Ok(OslLanSync {
        revision: state.revision,
        document: state.document.clone(),
        changed: state.revision != base_revision,
        conflict,
        connected: true,
    })
}

pub fn sync_guest(
    owner: &str,
    session_id: &str,
    base_revision: u64,
    document: Option<OslSharedDocument>,
) -> Result<OslLanSync, String> {
    let connection = joined()
        .lock()
        .map_err(|_| "The LAN session registry is unavailable".to_owned())?
        .get(session_id)
        .cloned()
        .ok_or_else(|| "That LAN session is no longer connected".to_owned())?;
    let mut client = connection
        .lock()
        .map_err(|_| "The LAN connection is unavailable".to_owned())?;
    if client.owner != owner {
        return Err("That LAN session belongs to another OSL identity".into());
    }
    if let Some(ref document) = document {
        validate_document(document)?;
    }
    let request = document.map_or(
        Request::Pull {
            known_revision: base_revision,
        },
        |document| Request::Push {
            base_revision,
            document,
        },
    );
    let room_id = client.room_id.clone();
    let secret = *client.secret;
    let request_sequence = client.request_sequence;
    send(
        &mut client.stream,
        &room_id,
        &secret,
        request_sequence,
        &request,
    )?;
    client.request_sequence += 1;
    let response_sequence = client.response_sequence;
    let response: Response = receive(&mut client.stream, &room_id, &secret, response_sequence)?;
    client.response_sequence += 1;
    let (revision, changed, conflict) = match response {
        Response::Snapshot {
            revision,
            document,
            conflict,
        } => {
            validate_document(&document)?;
            client.document = document;
            (revision, revision != base_revision, conflict)
        }
        Response::Unchanged { revision } => (revision, false, false),
    };
    client.revision = revision;
    Ok(OslLanSync {
        revision,
        document: client.document.clone(),
        changed,
        conflict,
        connected: true,
    })
}

pub fn stop(owner: &str, session_id: &str) -> Result<bool, String> {
    {
        let mut rooms = hosted()
            .lock()
            .map_err(|_| "The LAN room registry is unavailable".to_owned())?;
        if let Some(room) = rooms.get(session_id) {
            if room.owner != owner {
                return Err("That LAN room belongs to another OSL identity".into());
            }
        }
        if let Some(room) = rooms.remove(session_id) {
            room.stop.store(true, Ordering::Release);
            return Ok(true);
        }
    }
    let mut sessions = joined()
        .lock()
        .map_err(|_| "The LAN session registry is unavailable".to_owned())?;
    if sessions
        .get(session_id)
        .and_then(|session| session.lock().ok().map(|session| session.owner != owner))
        .unwrap_or(false)
    {
        return Err("That LAN session belongs to another OSL identity".into());
    }
    Ok(sessions.remove(session_id).is_some())
}

pub fn stop_all_for_owner(owner: &str) -> Result<usize, String> {
    let mut stopped = 0;
    {
        let mut rooms = hosted()
            .lock()
            .map_err(|_| "The LAN room registry is unavailable".to_owned())?;
        let ids = rooms
            .iter()
            .filter(|(_, room)| room.owner == owner)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            if let Some(room) = rooms.remove(&id) {
                room.stop.store(true, Ordering::Release);
                stopped += 1;
            }
        }
    }
    let mut sessions = joined()
        .lock()
        .map_err(|_| "The LAN session registry is unavailable".to_owned())?;
    let ids = sessions
        .iter()
        .filter_map(|(id, session)| {
            session
                .lock()
                .ok()
                .filter(|session| session.owner == owner)
                .map(|_| id.clone())
        })
        .collect::<Vec<_>>();
    for id in ids {
        if sessions.remove(&id).is_some() {
            stopped += 1;
        }
    }
    Ok(stopped)
}

fn serve(
    listener: TcpListener,
    room_id: String,
    secret: Zeroizing<[u8; 32]>,
    state: Arc<Mutex<RoomState>>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                let room = room_id.clone();
                let key = secret.clone();
                let shared = state.clone();
                let active = stop.clone();
                let _ = std::thread::Builder::new()
                    .name("osl-lan-peer".into())
                    .spawn(move || serve_peer(stream, &room, &key, shared, active));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(40))
            }
            Err(_) => break,
        }
    }
}

fn serve_peer(
    mut stream: TcpStream,
    room_id: &str,
    secret: &[u8; 32],
    state: Arc<Mutex<RoomState>>,
    active: Arc<AtomicBool>,
) {
    if configure_stream(&stream).is_err() {
        return;
    }
    let initial = {
        let Ok(state) = state.lock() else { return };
        Response::Snapshot {
            revision: state.revision,
            document: state.document.clone(),
            conflict: false,
        }
    };
    if send(&mut stream, room_id, secret, 0, &initial).is_err() {
        return;
    }
    let mut request_sequence = 0;
    let mut response_sequence = 1;
    while !active.load(Ordering::Acquire) {
        let Ok(request) = receive::<Request>(&mut stream, room_id, secret, request_sequence) else {
            break;
        };
        if active.load(Ordering::Acquire) {
            break;
        }
        request_sequence += 1;
        let response = {
            let Ok(mut state) = state.lock() else { break };
            match request {
                Request::Pull { known_revision } if known_revision == state.revision => {
                    Response::Unchanged {
                        revision: state.revision,
                    }
                }
                Request::Pull { .. } => Response::Snapshot {
                    revision: state.revision,
                    document: state.document.clone(),
                    conflict: false,
                },
                Request::Push {
                    base_revision,
                    document,
                } if validate_document(&document).is_ok() && base_revision == state.revision => {
                    state.revision = state.revision.saturating_add(1);
                    state.document = document.clone();
                    Response::Snapshot {
                        revision: state.revision,
                        document,
                        conflict: false,
                    }
                }
                Request::Push { .. } => Response::Snapshot {
                    revision: state.revision,
                    document: state.document.clone(),
                    conflict: true,
                },
            }
        };
        if send(&mut stream, room_id, secret, response_sequence, &response).is_err() {
            break;
        }
        response_sequence += 1;
    }
}

fn send<T: Serialize>(
    stream: &mut TcpStream,
    room_id: &str,
    secret: &[u8; 32],
    sequence: u64,
    value: &T,
) -> Result<(), String> {
    let plain = Zeroizing::new(
        serde_json::to_vec(value).map_err(|_| "The LAN update could not be encoded".to_owned())?,
    );
    let frame = seal_frame(room_id, sequence, secret, &plain)?;
    let wire = serde_json::to_vec(&frame)
        .map_err(|_| "The encrypted LAN frame could not be encoded".to_owned())?;
    if wire.len() > MAX_WIRE_BYTES {
        return Err("The encrypted LAN frame is too large".into());
    }
    stream
        .write_all(&(wire.len() as u32).to_be_bytes())
        .and_then(|_| stream.write_all(&wire))
        .map_err(|_| "The LAN connection was interrupted".to_owned())
}
fn receive<T: for<'de> Deserialize<'de>>(
    stream: &mut TcpStream,
    room_id: &str,
    secret: &[u8; 32],
    sequence: u64,
) -> Result<T, String> {
    let mut size = [0u8; 4];
    stream
        .read_exact(&mut size)
        .map_err(|_| "The LAN connection was interrupted".to_owned())?;
    let size = u32::from_be_bytes(size) as usize;
    if size == 0 || size > MAX_WIRE_BYTES {
        return Err("The LAN frame size is invalid".into());
    }
    let mut wire = vec![0u8; size];
    stream
        .read_exact(&mut wire)
        .map_err(|_| "The LAN connection was interrupted".to_owned())?;
    let frame = serde_json::from_slice::<OslCollaborationFrame>(&wire)
        .map_err(|_| "The encrypted LAN frame is malformed".to_owned())?;
    if frame.room_id != room_id {
        return Err("The LAN frame belongs to another room".into());
    }
    let plain = Zeroizing::new(open_frame(&frame, secret, sequence)?);
    serde_json::from_slice(&plain)
        .map_err(|_| "The authenticated LAN update is malformed".to_owned())
}
fn configure_stream(stream: &TcpStream) -> Result<(), String> {
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .and_then(|_| stream.set_write_timeout(Some(IO_TIMEOUT)))
        .and_then(|_| stream.set_nodelay(true))
        .map_err(|_| "The LAN connection could not be configured".to_owned())
}
fn local_ipv4() -> Result<std::net::IpAddr, String> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .map_err(|_| "A local network address is unavailable".to_owned())?;
    socket
        .connect("192.0.2.1:9")
        .map_err(|_| "A local network route is unavailable".to_owned())?;
    let ip = socket
        .local_addr()
        .map_err(|_| "A local network address is unavailable".to_owned())?
        .ip();
    if ip.is_unspecified() {
        Err("A usable local network address is unavailable".into())
    } else {
        Ok(ip)
    }
}
fn random_id(bytes: usize) -> String {
    crypto::random::random_bytes(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn validate_document(document: &OslSharedDocument) -> Result<(), String> {
    if document.title.as_bytes().len() > 240
        || document.body.as_bytes().len() > 256 * 1024
        || document.folder.as_bytes().len() > 80
        || document.tags.len() > 16
        || document
            .tags
            .iter()
            .any(|tag| tag.is_empty() || tag.as_bytes().len() > 32)
    {
        Err("That shared document exceeds OSL Notes limits".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document(title: &str) -> OslSharedDocument {
        OslSharedDocument {
            kind: OslDocumentKind::Document,
            title: title.into(),
            body: "Local collaboration".into(),
            folder: "Team".into(),
            tags: vec!["lan".into()],
            favorite: false,
        }
    }
    #[test]
    fn host_and_guest_exchange_authenticated_snapshots_without_a_relay() {
        let host = host("alice", document("Draft")).unwrap();
        let code = host.invitation.as_ref().unwrap().code.clone();
        let guest = join("bob", &code).unwrap();
        assert_eq!(guest.document.title, "Draft");
        let pushed = sync_guest("bob", &guest.session_id, 0, Some(document("Edited"))).unwrap();
        assert_eq!(pushed.revision, 1);
        let pulled = sync_host("alice", &host.session_id, 0, None).unwrap();
        assert!(pulled.changed);
        assert_eq!(pulled.document.title, "Edited");
        assert!(stop("bob", &guest.session_id).unwrap());
        assert!(stop("alice", &host.session_id).unwrap());
    }
}
