//! The scripting socket, client side: send one request line to the running fuxx and read the
//! reply (or, for `events` and `wait`, every line until the caller has what it needs).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::api::Request;

/// The socket a client talks to: `FUXX_SOCKET` when set and non-empty (inside a fuxx session),
/// else the one under the home folder.
pub fn client_socket_path(env_socket: Option<&str>, home: Option<&Path>) -> Option<PathBuf> {
    match env_socket.filter(|s| !s.is_empty()) {
        Some(socket) => Some(PathBuf::from(socket)),
        None => home.map(crate::api_server::socket_path),
    }
}

#[derive(Debug)]
pub enum ClientError {
    /// No fuxx answering at the socket (not running, or scripting off). Exit code 3.
    Unreachable(String),
    /// fuxx answered with an error reply.
    Api { code: String, message: String },
}

fn connect(path: &Path) -> Result<UnixStream, ClientError> {
    UnixStream::connect(path).map_err(|e| {
        ClientError::Unreachable(format!(
            "fuxx isn't running, or scripting is off ({}: {e})",
            path.display()
        ))
    })
}

fn lost(e: std::io::Error) -> ClientError {
    ClientError::Unreachable(format!("lost the connection to fuxx: {e}"))
}

/// Connect and send `request` as one line.
fn open(path: &Path, request: &Request) -> Result<BufReader<UnixStream>, ClientError> {
    let mut stream = connect(path)?;
    let mut line = serde_json::to_string(request).expect("a request always serializes");
    line.push('\n');
    stream.write_all(line.as_bytes()).map_err(lost)?;
    Ok(BufReader::new(stream))
}

/// A reply line → its `result`, or its `error` as `ClientError::Api`.
pub fn parse_reply(line: &str) -> Result<Value, ClientError> {
    let reply: Value = serde_json::from_str(line)
        .map_err(|e| ClientError::Unreachable(format!("fuxx sent a line that isn't JSON: {e}")))?;
    if let Some(error) = reply.get("error") {
        let text = |key: &str| error[key].as_str().unwrap_or_default().to_string();
        return Err(ClientError::Api {
            code: text("code"),
            message: text("message"),
        });
    }
    Ok(reply.get("result").cloned().unwrap_or(Value::Null))
}

/// Send one request and wait for its reply.
pub fn call(path: &Path, request: &Request) -> Result<Value, ClientError> {
    let mut reader = open(path, request)?;
    let mut line = String::new();
    if reader.read_line(&mut line).map_err(lost)? == 0 {
        return Err(ClientError::Unreachable(
            "fuxx closed the connection without replying".into(),
        ));
    }
    parse_reply(&line)
}

/// Send one request, then hand every line (without its newline) to `on_line` until it returns
/// false or fuxx closes the connection.
pub fn stream(
    path: &Path,
    request: &Request,
    mut on_line: impl FnMut(&str) -> bool,
) -> Result<(), ClientError> {
    let reader = open(path, request)?;
    for line in reader.lines() {
        if !on_line(&line.map_err(lost)?) {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuxx_socket_wins_over_home() {
        let home = Path::new("/Users/me");
        let via_home = PathBuf::from("/Users/me/Library/Application Support/fuxx/fuxx.sock");
        assert_eq!(
            client_socket_path(Some("/tmp/f.sock"), Some(home)),
            Some(PathBuf::from("/tmp/f.sock"))
        );
        assert_eq!(
            client_socket_path(Some(""), Some(home)),
            Some(via_home.clone())
        );
        assert_eq!(client_socket_path(None, Some(home)), Some(via_home));
        assert_eq!(client_socket_path(None, None), None);
        assert_eq!(client_socket_path(Some(""), None), None);
    }
}
