//! M10a: the wire format between the GUI and the background `fuxx daemon` (study §2.3.3).
//! Pure: no I/O, no threads, no gpui.
//!
//! A frame is `u32 BE length | u8 kind | payload`. `length` counts the kind byte and the
//! payload, and may not exceed [`MAX_FRAME`]. Hot-path frames (`Input`, `Output`, `Snapshot`)
//! have a fixed binary header followed by raw bytes, so PTY output is never base64'd. Every
//! other kind is a JSON object. Unknown JSON fields are ignored, so a newer peer can add
//! fields; an unknown kind byte decodes to [`Frame::Unknown`] so a newer peer can add kinds.

use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;

/// Oldest and newest protocol version this build speaks.
pub const PROTO_MIN: u32 = 1;
pub const PROTO_MAX: u32 = 1;
/// Largest accepted `length` field (kind byte plus payload).
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum ProtoError {
    /// The length field is zero or above `MAX_FRAME`. The stream is unrecoverable.
    BadLength(usize),
    /// A binary payload is shorter than its header says.
    Malformed(&'static str),
    /// A JSON payload didn't parse as its kind.
    Json(String),
    /// No protocol version in common.
    Incompatible {
        ours: (u32, u32),
        theirs: (u32, u32),
    },
}

impl std::fmt::Display for ProtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadLength(n) => write!(f, "bad frame length {n}"),
            Self::Malformed(what) => write!(f, "malformed frame: {what}"),
            Self::Json(e) => write!(f, "bad frame JSON: {e}"),
            Self::Incompatible { ours, theirs } => write!(
                f,
                "no common protocol version (ours {}..={}, theirs {}..={})",
                ours.0, ours.1, theirs.0, theirs.1
            ),
        }
    }
}

impl std::error::Error for ProtoError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dims {
    pub cols: u16,
    pub rows: u16,
}

/// Cell size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellPx {
    pub w: u16,
    pub h: u16,
}

/// A session's progress bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub percent: u8,
    pub label: Option<String>,
}

/// Tri-state field: absent = leave (`None`), `null` = clear (`Some(None)`), value = set.
fn tri<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Ui,
    Script,
    /// A role from a newer peer.
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub proto_min: u32,
    pub proto_max: u32,
    pub app_version: String,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelloOk {
    pub proto: u32,
    pub daemon_version: String,
    pub pid: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IncompatibleInfo {
    pub proto_min: u32,
    pub proto_max: u32,
}

/// One entry of `Sessions`: enough to draw the sidebar before attaching anything.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: u64,
    pub name: String,
    pub project: Option<String>,
    pub preset: Option<String>,
    pub title: Option<String>,
    pub status: String,
    pub note: Option<String>,
    pub progress: Option<Progress>,
    pub dims: Dims,
    pub alive: bool,
    /// Unix seconds.
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Create {
    pub cwd: Option<String>,
    pub startup: Option<String>,
    /// The GUI's environment plus the `FUXX_*` scripting variables.
    pub env: BTreeMap<String, String>,
    pub dims: Dims,
    pub cell_px: CellPx,
    pub name: Option<String>,
    pub project: Option<String>,
    pub preset: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resize {
    pub id: u64,
    pub cols: u16,
    pub rows: u16,
    pub cell_w: u16,
    pub cell_h: u16,
}

/// Per field: absent leaves it as it is, `null` clears it, a value sets it
/// (`None` / `Some(None)` / `Some(Some(v))`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetMeta {
    pub id: u64,
    #[serde(
        default,
        deserialize_with = "tri",
        skip_serializing_if = "Option::is_none"
    )]
    pub name: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "tri",
        skip_serializing_if = "Option::is_none"
    )]
    pub project: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "tri",
        skip_serializing_if = "Option::is_none"
    )]
    pub preset: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "tri",
        skip_serializing_if = "Option::is_none"
    )]
    pub note: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "tri",
        skip_serializing_if = "Option::is_none"
    )]
    pub progress: Option<Option<Progress>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadText {
    pub id: u64,
    pub scope: String,
}

/// Reply to `ReadText` (not in the study's table; a request with no reply is useless).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub id: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resized {
    pub id: u64,
    pub seq: u64,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: u64,
    pub seq: u64,
    pub dims: Dims,
    /// The window title; it isn't in the VT bytes.
    pub title: Option<String>,
    pub vt_bytes: Vec<u8>,
}

/// Reply to `Info` (`fuxx daemon status`; not in the study's table).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DaemonInfo {
    pub pid: u32,
    pub version: String,
    pub proto: u32,
    pub uptime_secs: u64,
    pub sessions: u64,
}

/// Reply to `Stop` (`fuxx daemon stop`): `stopping` false means refused because of `sessions`;
/// true means the daemon hung them up (if any) and is exiting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StopReply {
    pub stopping: bool,
    pub sessions: Vec<SessionInfo>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    // Client to daemon.
    Hello(Hello),
    List,
    Create(Create),
    Attach {
        id: u64,
    },
    Detach {
        id: u64,
    },
    Input {
        id: u64,
        bytes: Vec<u8>,
    },
    Resize(Resize),
    SetTheme {
        palette: serde_json::Value,
    },
    SetMeta(SetMeta),
    Close {
        id: u64,
    },
    ReadText(ReadText),
    /// Ask for `DaemonInfo`.
    Info,
    /// End the daemon; without `force` it refuses while sessions are alive.
    Stop {
        force: bool,
    },
    // Daemon to client.
    HelloOk(HelloOk),
    Incompatible(IncompatibleInfo),
    Sessions(Vec<SessionInfo>),
    Created {
        id: u64,
    },
    Snapshot(Snapshot),
    Output {
        id: u64,
        seq: u64,
        notifications: u32,
        bytes: Vec<u8>,
    },
    Resized(Resized),
    Status {
        id: u64,
        status: String,
    },
    Title {
        id: u64,
        title: Option<String>,
    },
    Bell {
        id: u64,
    },
    Exited {
        id: u64,
    },
    Resync {
        id: u64,
    },
    Text(Text),
    DaemonInfo(DaemonInfo),
    StopReply(StopReply),
    /// A request failed (`id`: the session it was about, if any).
    Error {
        id: Option<u64>,
        message: String,
    },
    /// A kind this build doesn't know; skip it.
    Unknown {
        kind: u8,
    },
}

mod kind {
    pub const HELLO: u8 = 0x01;
    pub const HELLO_OK: u8 = 0x02;
    pub const INCOMPATIBLE: u8 = 0x03;
    pub const LIST: u8 = 0x04;
    pub const SESSIONS: u8 = 0x05;
    pub const CREATE: u8 = 0x06;
    pub const CREATED: u8 = 0x07;
    pub const ATTACH: u8 = 0x08;
    pub const DETACH: u8 = 0x09;
    pub const INPUT: u8 = 0x0A;
    pub const RESIZE: u8 = 0x0B;
    pub const SET_THEME: u8 = 0x0C;
    pub const SET_META: u8 = 0x0D;
    pub const CLOSE: u8 = 0x0E;
    pub const READ_TEXT: u8 = 0x0F;
    pub const SNAPSHOT: u8 = 0x10;
    pub const OUTPUT: u8 = 0x11;
    pub const STATUS: u8 = 0x12;
    pub const TITLE: u8 = 0x13;
    pub const BELL: u8 = 0x14;
    pub const EXITED: u8 = 0x15;
    pub const RESYNC: u8 = 0x16;
    pub const RESIZED: u8 = 0x17;
    pub const TEXT: u8 = 0x18;
    pub const INFO: u8 = 0x19;
    pub const DAEMON_INFO: u8 = 0x1A;
    pub const STOP: u8 = 0x1B;
    pub const STOP_REPLY: u8 = 0x1C;
    pub const ERROR: u8 = 0x1D;
}

#[derive(Serialize, Deserialize)]
struct IdOnly {
    id: u64,
}
#[derive(Serialize, Deserialize)]
struct StatusBody {
    id: u64,
    status: String,
}
#[derive(Serialize, Deserialize)]
struct TitleBody {
    id: u64,
    title: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct ThemeBody {
    palette: serde_json::Value,
}
#[derive(Serialize, Deserialize)]
struct SnapshotMeta {
    dims: Dims,
    title: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct StopBody {
    force: bool,
}
#[derive(Serialize, Deserialize)]
struct ErrorBody {
    id: Option<u64>,
    message: String,
}
#[derive(Serialize, Deserialize)]
struct Empty {}

/// Highest protocol version both sides speak.
pub fn negotiate(client_min: u32, client_max: u32) -> Result<u32, ProtoError> {
    let hi = client_max.min(PROTO_MAX);
    if client_min <= client_max && hi >= client_min.max(PROTO_MIN) {
        Ok(hi)
    } else {
        Err(ProtoError::Incompatible {
            ours: (PROTO_MIN, PROTO_MAX),
            theirs: (client_min, client_max),
        })
    }
}

fn json<T: Serialize>(k: u8, body: &T) -> (u8, Vec<u8>) {
    (k, serde_json::to_vec(body).expect("frame bodies serialize"))
}

fn id_only(k: u8, id: u64) -> (u8, Vec<u8>) {
    json(k, &IdOnly { id })
}

/// Encode one frame, length prefix included.
///
/// Callers keep `Input`, `Output` and `Snapshot` bytes small enough that the frame fits in
/// [`MAX_FRAME`]; the decoder rejects anything larger.
pub fn encode(frame: &Frame) -> Vec<u8> {
    let (k, payload) = match frame {
        Frame::Hello(b) => json(kind::HELLO, b),
        Frame::List => json(kind::LIST, &Empty {}),
        Frame::Create(b) => json(kind::CREATE, b),
        Frame::Attach { id } => id_only(kind::ATTACH, *id),
        Frame::Detach { id } => id_only(kind::DETACH, *id),
        Frame::Input { id, bytes } => {
            let mut p = Vec::with_capacity(8 + bytes.len());
            p.extend_from_slice(&id.to_be_bytes());
            p.extend_from_slice(bytes);
            (kind::INPUT, p)
        }
        Frame::Resize(b) => json(kind::RESIZE, b),
        Frame::SetTheme { palette } => json(
            kind::SET_THEME,
            &ThemeBody {
                palette: palette.clone(),
            },
        ),
        Frame::SetMeta(b) => json(kind::SET_META, b),
        Frame::Close { id } => id_only(kind::CLOSE, *id),
        Frame::ReadText(b) => json(kind::READ_TEXT, b),
        Frame::HelloOk(b) => json(kind::HELLO_OK, b),
        Frame::Incompatible(b) => json(kind::INCOMPATIBLE, b),
        Frame::Sessions(b) => json(kind::SESSIONS, b),
        Frame::Created { id } => id_only(kind::CREATED, *id),
        Frame::Snapshot(s) => {
            let meta = serde_json::to_vec(&SnapshotMeta {
                dims: s.dims,
                title: s.title.clone(),
            })
            .expect("snapshot meta serializes");
            let mut p = Vec::with_capacity(20 + meta.len() + s.vt_bytes.len());
            p.extend_from_slice(&s.id.to_be_bytes());
            p.extend_from_slice(&s.seq.to_be_bytes());
            p.extend_from_slice(&(meta.len() as u32).to_be_bytes());
            p.extend_from_slice(&meta);
            p.extend_from_slice(&s.vt_bytes);
            (kind::SNAPSHOT, p)
        }
        Frame::Output {
            id,
            seq,
            notifications,
            bytes,
        } => {
            let mut p = Vec::with_capacity(20 + bytes.len());
            p.extend_from_slice(&id.to_be_bytes());
            p.extend_from_slice(&seq.to_be_bytes());
            p.extend_from_slice(&notifications.to_be_bytes());
            p.extend_from_slice(bytes);
            (kind::OUTPUT, p)
        }
        Frame::Resized(b) => json(kind::RESIZED, b),
        Frame::Status { id, status } => json(
            kind::STATUS,
            &StatusBody {
                id: *id,
                status: status.clone(),
            },
        ),
        Frame::Title { id, title } => json(
            kind::TITLE,
            &TitleBody {
                id: *id,
                title: title.clone(),
            },
        ),
        Frame::Bell { id } => id_only(kind::BELL, *id),
        Frame::Exited { id } => id_only(kind::EXITED, *id),
        Frame::Resync { id } => id_only(kind::RESYNC, *id),
        Frame::Text(b) => json(kind::TEXT, b),
        Frame::Info => json(kind::INFO, &Empty {}),
        Frame::DaemonInfo(b) => json(kind::DAEMON_INFO, b),
        Frame::Stop { force } => json(kind::STOP, &StopBody { force: *force }),
        Frame::StopReply(b) => json(kind::STOP_REPLY, b),
        Frame::Error { id, message } => json(
            kind::ERROR,
            &ErrorBody {
                id: *id,
                message: message.clone(),
            },
        ),
        Frame::Unknown { kind } => (*kind, Vec::new()),
    };
    debug_assert!(payload.len() < MAX_FRAME, "frame over MAX_FRAME");
    let len = u32::try_from(payload.len() + 1).expect("frame length fits u32");
    let mut out = Vec::with_capacity(5 + payload.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.push(k);
    out.extend_from_slice(&payload);
    out
}

fn parse<T: DeserializeOwned>(payload: &[u8]) -> Result<T, ProtoError> {
    serde_json::from_slice(payload).map_err(|e| ProtoError::Json(e.to_string()))
}

fn be_u64(b: &[u8]) -> u64 {
    u64::from_be_bytes(b[..8].try_into().expect("caller checked length"))
}

fn be_u32(b: &[u8]) -> u32 {
    u32::from_be_bytes(b[..4].try_into().expect("caller checked length"))
}

fn decode_frame(k: u8, p: &[u8]) -> Result<Frame, ProtoError> {
    Ok(match k {
        kind::HELLO => Frame::Hello(parse(p)?),
        kind::HELLO_OK => Frame::HelloOk(parse(p)?),
        kind::INCOMPATIBLE => Frame::Incompatible(parse(p)?),
        kind::LIST => {
            parse::<Empty>(p)?;
            Frame::List
        }
        kind::SESSIONS => Frame::Sessions(parse(p)?),
        kind::CREATE => Frame::Create(parse(p)?),
        kind::CREATED => Frame::Created {
            id: parse::<IdOnly>(p)?.id,
        },
        kind::ATTACH => Frame::Attach {
            id: parse::<IdOnly>(p)?.id,
        },
        kind::DETACH => Frame::Detach {
            id: parse::<IdOnly>(p)?.id,
        },
        kind::INPUT => {
            if p.len() < 8 {
                return Err(ProtoError::Malformed("Input header"));
            }
            Frame::Input {
                id: be_u64(p),
                bytes: p[8..].to_vec(),
            }
        }
        kind::RESIZE => Frame::Resize(parse(p)?),
        kind::SET_THEME => Frame::SetTheme {
            palette: parse::<ThemeBody>(p)?.palette,
        },
        kind::SET_META => Frame::SetMeta(parse(p)?),
        kind::CLOSE => Frame::Close {
            id: parse::<IdOnly>(p)?.id,
        },
        kind::READ_TEXT => Frame::ReadText(parse(p)?),
        kind::SNAPSHOT => {
            if p.len() < 20 {
                return Err(ProtoError::Malformed("Snapshot header"));
            }
            let meta_len = be_u32(&p[16..]) as usize;
            let rest = &p[20..];
            if rest.len() < meta_len {
                return Err(ProtoError::Malformed("Snapshot metadata"));
            }
            let meta: SnapshotMeta = parse(&rest[..meta_len])?;
            Frame::Snapshot(Snapshot {
                id: be_u64(p),
                seq: be_u64(&p[8..]),
                dims: meta.dims,
                title: meta.title,
                vt_bytes: rest[meta_len..].to_vec(),
            })
        }
        kind::OUTPUT => {
            if p.len() < 20 {
                return Err(ProtoError::Malformed("Output header"));
            }
            Frame::Output {
                id: be_u64(p),
                seq: be_u64(&p[8..]),
                notifications: be_u32(&p[16..]),
                bytes: p[20..].to_vec(),
            }
        }
        kind::RESIZED => Frame::Resized(parse(p)?),
        kind::STATUS => {
            let b: StatusBody = parse(p)?;
            Frame::Status {
                id: b.id,
                status: b.status,
            }
        }
        kind::TITLE => {
            let b: TitleBody = parse(p)?;
            Frame::Title {
                id: b.id,
                title: b.title,
            }
        }
        kind::BELL => Frame::Bell {
            id: parse::<IdOnly>(p)?.id,
        },
        kind::EXITED => Frame::Exited {
            id: parse::<IdOnly>(p)?.id,
        },
        kind::RESYNC => Frame::Resync {
            id: parse::<IdOnly>(p)?.id,
        },
        kind::TEXT => Frame::Text(parse(p)?),
        kind::INFO => {
            parse::<Empty>(p)?;
            Frame::Info
        }
        kind::DAEMON_INFO => Frame::DaemonInfo(parse(p)?),
        kind::STOP => Frame::Stop {
            force: parse::<StopBody>(p)?.force,
        },
        kind::STOP_REPLY => Frame::StopReply(parse(p)?),
        kind::ERROR => {
            let b: ErrorBody = parse(p)?;
            Frame::Error {
                id: b.id,
                message: b.message,
            }
        }
        other => Frame::Unknown { kind: other },
    })
}

/// Incremental frame decoder: `push` bytes as they arrive, then drain with `next`.
#[derive(Default)]
pub struct Decoder {
    buf: Vec<u8>,
    pos: usize,
    /// A bad length desyncs the stream for good; keep reporting it.
    fatal: Option<ProtoError>,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        if self.fatal.is_some() {
            return; // desynced; don't buffer more
        }
        if self.pos > 0 {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        self.buf.extend_from_slice(bytes);
    }

    /// The next complete frame, `None` if more bytes are needed. A bad payload is an `Err`
    /// for that frame only; the following frame still decodes. A bad length is fatal.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Result<Frame, ProtoError>> {
        if let Some(e) = &self.fatal {
            return Some(Err(e.clone()));
        }
        let avail = &self.buf[self.pos..];
        if avail.len() < 4 {
            return None;
        }
        let len = be_u32(avail) as usize;
        if len == 0 || len > MAX_FRAME {
            let e = ProtoError::BadLength(len);
            self.fatal = Some(e.clone());
            return Some(Err(e));
        }
        if avail.len() < 4 + len {
            return None;
        }
        let k = avail[4];
        let result = decode_frame(k, &avail[5..4 + len]);
        self.pos += 4 + len;
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dims() -> Dims {
        Dims { cols: 80, rows: 24 }
    }

    fn info() -> SessionInfo {
        SessionInfo {
            id: 3,
            name: "agent".into(),
            project: Some("fuxx".into()),
            preset: None,
            title: Some("vim".into()),
            status: "running".into(),
            note: Some("n".into()),
            progress: Some(Progress {
                percent: 40,
                label: Some("half".into()),
            }),
            dims: dims(),
            alive: true,
            created_at: 1_700_000_000,
        }
    }

    fn all_frames() -> Vec<Frame> {
        let mut env = BTreeMap::new();
        env.insert("PATH".to_string(), "/usr/bin".to_string());
        vec![
            Frame::Hello(Hello {
                proto_min: 1,
                proto_max: 2,
                app_version: "0.1".into(),
                role: Role::Ui,
            }),
            Frame::List,
            Frame::Create(Create {
                cwd: Some("/tmp".into()),
                startup: None,
                env,
                dims: dims(),
                cell_px: CellPx { w: 9, h: 18 },
                name: Some("a".into()),
                project: None,
                preset: Some("claude".into()),
            }),
            Frame::Attach { id: 7 },
            Frame::Detach { id: 7 },
            Frame::Input {
                id: 7,
                bytes: vec![0, 1, 255, b'\n'],
            },
            Frame::Resize(Resize {
                id: 7,
                cols: 100,
                rows: 30,
                cell_w: 9,
                cell_h: 18,
            }),
            Frame::SetTheme {
                palette: serde_json::json!({"fg": "#fff", "ansi": [1, 2]}),
            },
            Frame::SetMeta(SetMeta {
                id: 7,
                name: Some(Some("n".into())),
                project: None,
                preset: Some(None),
                note: Some(Some("x".into())),
                progress: Some(Some(Progress {
                    percent: 10,
                    label: None,
                })),
            }),
            Frame::Close { id: 7 },
            Frame::ReadText(ReadText {
                id: 7,
                scope: "screen".into(),
            }),
            Frame::HelloOk(HelloOk {
                proto: 1,
                daemon_version: "0.1".into(),
                pid: 42,
            }),
            Frame::Incompatible(IncompatibleInfo {
                proto_min: 1,
                proto_max: 1,
            }),
            Frame::Sessions(vec![info()]),
            Frame::Sessions(vec![]),
            Frame::Created { id: 9 },
            Frame::Snapshot(Snapshot {
                id: 7,
                seq: u64::MAX,
                dims: dims(),
                title: Some("t".into()),
                vt_bytes: (0..=255).collect(),
            }),
            Frame::Output {
                id: 7,
                seq: 12,
                notifications: 2,
                bytes: b"\x1b[31mhi\x00".to_vec(),
            },
            Frame::Output {
                id: 7,
                seq: 13,
                notifications: 0,
                bytes: vec![],
            },
            Frame::Resized(Resized {
                id: 7,
                seq: 14,
                cols: 90,
                rows: 20,
            }),
            Frame::Status {
                id: 7,
                status: "waiting".into(),
            },
            Frame::Title { id: 7, title: None },
            Frame::Title {
                id: 7,
                title: Some("x".into()),
            },
            Frame::Bell { id: 7 },
            Frame::Exited { id: 7 },
            Frame::Resync { id: 7 },
            Frame::Text(Text {
                id: 7,
                text: "hello\n".into(),
            }),
            Frame::Info,
            Frame::DaemonInfo(DaemonInfo {
                pid: 42,
                version: "0.2.0".into(),
                proto: 1,
                uptime_secs: 75,
                sessions: 3,
            }),
            Frame::Stop { force: true },
            Frame::Stop { force: false },
            Frame::StopReply(StopReply {
                stopping: false,
                sessions: vec![info()],
            }),
            Frame::Error {
                id: Some(7),
                message: "no such session".into(),
            },
            Frame::Error {
                id: None,
                message: "expected Hello".into(),
            },
        ]
    }

    fn drain(d: &mut Decoder) -> Vec<Result<Frame, ProtoError>> {
        std::iter::from_fn(|| d.next()).collect()
    }

    #[test]
    fn every_frame_kind_round_trips() {
        for f in all_frames() {
            let mut d = Decoder::new();
            d.push(&encode(&f));
            assert_eq!(d.next(), Some(Ok(f.clone())), "{f:?}");
            assert_eq!(d.next(), None);
        }
    }

    #[test]
    fn split_across_many_pushes_decodes_once() {
        let f = Frame::Output {
            id: 1,
            seq: 2,
            notifications: 0,
            bytes: vec![7; 300],
        };
        let bytes = encode(&f);
        let mut d = Decoder::new();
        let mut got = vec![];
        for b in &bytes {
            d.push(std::slice::from_ref(b));
            got.extend(drain(&mut d));
        }
        assert_eq!(got, vec![Ok(f)]);
    }

    #[test]
    fn two_frames_in_one_push() {
        let (a, b) = (Frame::Attach { id: 1 }, Frame::Bell { id: 2 });
        let mut bytes = encode(&a);
        bytes.extend(encode(&b));
        let mut d = Decoder::new();
        d.push(&bytes);
        assert_eq!(drain(&mut d), vec![Ok(a), Ok(b)]);
    }

    #[test]
    fn oversize_length_is_an_error_and_stays_one() {
        let mut d = Decoder::new();
        d.push(&((MAX_FRAME as u32) + 1).to_be_bytes());
        assert_eq!(d.next(), Some(Err(ProtoError::BadLength(MAX_FRAME + 1))));
        d.push(&[0; 8]);
        assert!(matches!(d.next(), Some(Err(ProtoError::BadLength(_)))));
    }

    #[test]
    fn max_size_length_is_accepted_but_zero_is_not() {
        let mut d = Decoder::new();
        d.push(&(MAX_FRAME as u32).to_be_bytes());
        assert_eq!(d.next(), None, "waits for the body");
        let mut z = Decoder::new();
        z.push(&0u32.to_be_bytes());
        assert_eq!(z.next(), Some(Err(ProtoError::BadLength(0))));
    }

    #[test]
    fn unknown_kind_is_unknown_frame_and_stream_continues() {
        let mut bytes = vec![0, 0, 0, 4, 0xEE, 1, 2, 3];
        bytes.extend(encode(&Frame::Bell { id: 5 }));
        let mut d = Decoder::new();
        d.push(&bytes);
        assert_eq!(
            drain(&mut d),
            vec![Ok(Frame::Unknown { kind: 0xEE }), Ok(Frame::Bell { id: 5 })]
        );
    }

    #[test]
    fn json_with_extra_fields_decodes() {
        let body = br#"{"id":3,"future":{"a":1}}"#;
        let mut bytes = ((body.len() + 1) as u32).to_be_bytes().to_vec();
        bytes.push(kind::ATTACH);
        bytes.extend_from_slice(body);
        let hello =
            br#"{"proto_min":1,"proto_max":1,"app_version":"x","role":"robot","extra":true}"#;
        bytes.extend(((hello.len() + 1) as u32).to_be_bytes());
        bytes.push(kind::HELLO);
        bytes.extend_from_slice(hello);
        let mut d = Decoder::new();
        d.push(&bytes);
        assert_eq!(
            drain(&mut d),
            vec![
                Ok(Frame::Attach { id: 3 }),
                Ok(Frame::Hello(Hello {
                    proto_min: 1,
                    proto_max: 1,
                    app_version: "x".into(),
                    role: Role::Other
                })),
            ]
        );
    }

    #[test]
    fn bad_payload_errors_that_frame_only() {
        let mut bytes = vec![0, 0, 0, 4, kind::ATTACH, b'n', b'o', b'!'];
        bytes.extend([0, 0, 0, 3, kind::INPUT, 1, 2]); // Input shorter than its header
        bytes.extend(encode(&Frame::Bell { id: 1 }));
        let mut d = Decoder::new();
        d.push(&bytes);
        let got = drain(&mut d);
        assert!(matches!(got[0], Err(ProtoError::Json(_))));
        assert!(matches!(got[1], Err(ProtoError::Malformed(_))));
        assert_eq!(got[2], Ok(Frame::Bell { id: 1 }));
    }

    #[test]
    fn binary_layouts_are_as_documented() {
        let out = encode(&Frame::Output {
            id: 1,
            seq: 2,
            notifications: 3,
            bytes: b"ab".to_vec(),
        });
        let mut want = vec![0, 0, 0, 23, kind::OUTPUT];
        want.extend(1u64.to_be_bytes());
        want.extend(2u64.to_be_bytes());
        want.extend(3u32.to_be_bytes());
        want.extend(b"ab");
        assert_eq!(out, want);
        let inp = encode(&Frame::Input {
            id: 1,
            bytes: b"x".to_vec(),
        });
        assert_eq!(
            inp,
            [
                vec![0, 0, 0, 10, kind::INPUT],
                1u64.to_be_bytes().to_vec(),
                b"x".to_vec()
            ]
            .concat()
        );
    }

    #[test]
    fn negotiation_picks_highest_common_or_incompatible() {
        assert_eq!(negotiate(1, 1), Ok(1));
        assert_eq!(negotiate(0, 5), Ok(PROTO_MAX));
        assert_eq!(negotiate(PROTO_MAX, PROTO_MAX + 3), Ok(PROTO_MAX));
        assert!(matches!(
            negotiate(PROTO_MAX + 1, PROTO_MAX + 2),
            Err(ProtoError::Incompatible { .. })
        ));
        assert!(
            matches!(negotiate(5, 2), Err(ProtoError::Incompatible { .. })),
            "inverted range"
        );
    }

    fn raw(k: u8, payload: &[u8]) -> Vec<u8> {
        let mut v = ((payload.len() + 1) as u32).to_be_bytes().to_vec();
        v.push(k);
        v.extend_from_slice(payload);
        v
    }

    fn one(bytes: &[u8]) -> Result<Frame, ProtoError> {
        let mut d = Decoder::new();
        d.push(bytes);
        d.next().unwrap()
    }

    #[test]
    fn set_meta_absent_leaves_null_clears_value_sets() {
        let m = |j: &str| match one(&raw(kind::SET_META, j.as_bytes())) {
            Ok(Frame::SetMeta(m)) => m,
            other => panic!("{other:?}"),
        };
        let leave = m(r#"{"id":1}"#);
        assert_eq!(
            (&leave.name, &leave.note, &leave.progress),
            (&None, &None, &None)
        );
        let clear = m(r#"{"id":1,"name":null,"note":null,"progress":null}"#);
        assert_eq!(
            (clear.name, clear.note, clear.progress),
            (Some(None), Some(None), Some(None))
        );
        let set = m(r#"{"id":1,"name":"a","note":"b","progress":{"percent":5,"label":null}}"#);
        assert_eq!(set.name, Some(Some("a".into())));
        assert_eq!(set.note, Some(Some("b".into())));
        assert_eq!(
            set.progress,
            Some(Some(Progress {
                percent: 5,
                label: None
            }))
        );
    }

    #[test]
    fn object_forms_on_the_wire() {
        let f = encode(&Frame::Create(Create {
            cwd: None,
            startup: None,
            env: BTreeMap::new(),
            dims: dims(),
            cell_px: CellPx { w: 9, h: 18 },
            name: None,
            project: None,
            preset: None,
        }));
        assert!(String::from_utf8_lossy(&f).contains(r#""cell_px":{"w":9,"h":18}"#));
    }

    #[test]
    fn fatal_decoder_stops_buffering() {
        let mut d = Decoder::new();
        d.push(&0u32.to_be_bytes());
        assert!(d.next().is_some());
        d.push(&[1; 1000]);
        assert!(d.buf.len() <= 4);
        assert!(matches!(d.next(), Some(Err(ProtoError::BadLength(0)))));
    }

    #[test]
    fn short_binary_headers_are_malformed() {
        assert!(matches!(
            one(&raw(kind::SNAPSHOT, &[0; 19])),
            Err(ProtoError::Malformed(_))
        ));
        assert!(matches!(
            one(&raw(kind::OUTPUT, &[0; 19])),
            Err(ProtoError::Malformed(_))
        ));
        // meta_len claims more than the frame holds
        let mut p = vec![0; 16];
        p.extend(100u32.to_be_bytes());
        p.extend(b"{}");
        assert!(matches!(
            one(&raw(kind::SNAPSHOT, &p)),
            Err(ProtoError::Malformed(_))
        ));
    }

    #[test]
    fn snapshot_byte_layout_is_pinned() {
        let meta = br#"{"dims":{"cols":80,"rows":24},"title":null}"#;
        let got = encode(&Frame::Snapshot(Snapshot {
            id: 1,
            seq: 2,
            dims: dims(),
            title: None,
            vt_bytes: b"vt".to_vec(),
        }));
        let mut want = ((1 + 20 + meta.len() + 2) as u32).to_be_bytes().to_vec();
        want.push(kind::SNAPSHOT);
        want.extend(1u64.to_be_bytes());
        want.extend(2u64.to_be_bytes());
        want.extend((meta.len() as u32).to_be_bytes());
        want.extend(meta);
        want.extend(b"vt");
        assert_eq!(got, want);
    }
}
