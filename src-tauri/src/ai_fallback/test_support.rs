//! Minimal in-process HTTP server for tests (std `TcpListener`, no dependencies).
//!
//! It records every request, answers through a caller-supplied handler and shuts down on
//! drop. Used to stand in for an Ollama server and for the model registry.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Debug, Clone)]
pub(crate) struct MockRequest {
    pub method: String,
    pub path: String,
    pub body: String,
}

impl MockRequest {
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

pub(crate) struct MockReply {
    pub status: u16,
    pub body: String,
    pub delay: Duration,
    /// Extra response headers (e.g. `Retry-After`, `X-RateLimit-Reset`).
    pub headers: Vec<(String, String)>,
    /// Send only this many body bytes but announce the full `Content-Length`, then hang up:
    /// a connection that drops in the middle of a download.
    pub cut_body_at: Option<usize>,
    /// With `cut_body_at`: keep the connection open this long before hanging up (a stall).
    pub hold_open: Duration,
    /// Close the connection without answering at all.
    pub hang_up: bool,
}

impl MockReply {
    pub fn ok(body: serde_json::Value) -> Self {
        Self {
            status: 200,
            body: body.to_string(),
            delay: Duration::ZERO,
            headers: Vec::new(),
            cut_body_at: None,
            hold_open: Duration::ZERO,
            hang_up: false,
        }
    }

    /// Raw text body (not JSON-encoded), e.g. broken JSON or a stand-in file.
    pub fn text(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.to_string(),
            delay: Duration::ZERO,
            headers: Vec::new(),
            cut_body_at: None,
            hold_open: Duration::ZERO,
            hang_up: false,
        }
    }

    pub fn empty_ok() -> Self {
        Self {
            status: 200,
            body: String::new(),
            delay: Duration::ZERO,
            headers: Vec::new(),
            cut_body_at: None,
            hold_open: Duration::ZERO,
            hang_up: false,
        }
    }

    pub fn error(status: u16, message: &str) -> Self {
        Self {
            status,
            body: serde_json::json!({ "error": message }).to_string(),
            delay: Duration::ZERO,
            headers: Vec::new(),
            cut_body_at: None,
            hold_open: Duration::ZERO,
            hang_up: false,
        }
    }

    pub fn delayed(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    pub fn cut_at(mut self, bytes: usize) -> Self {
        self.cut_body_at = Some(bytes);
        self
    }

    /// The server accepts the connection and closes it without a word.
    pub fn hang_up() -> Self {
        Self {
            hang_up: true,
            ..Self::empty_ok()
        }
    }

    pub fn then_stall(mut self, hold_open: Duration) -> Self {
        self.hold_open = hold_open;
        self
    }
}

type Handler = dyn Fn(&MockRequest) -> MockReply + Send + Sync + 'static;

pub(crate) struct MockServer {
    pub base_url: String,
    port: u16,
    requests: Arc<Mutex<Vec<MockRequest>>>,
    stop: Arc<AtomicBool>,
    accept_thread: Option<JoinHandle<()>>,
}

impl MockServer {
    pub fn start(handler: impl Fn(&MockRequest) -> MockReply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let port = listener.local_addr().expect("mock addr").port();
        let handler: Arc<Handler> = Arc::new(handler);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let accept_requests = requests.clone();
        let accept_stop = stop.clone();
        let accept_thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if accept_stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let handler = handler.clone();
                let requests = accept_requests.clone();
                std::thread::spawn(move || serve_connection(stream, &*handler, &requests));
            }
        });

        Self {
            base_url: format!("http://127.0.0.1:{}", port),
            port,
            requests,
            stop,
            accept_thread: Some(accept_thread),
        }
    }

    pub fn requests(&self) -> Vec<MockRequest> {
        self.requests.lock().expect("request log").clone()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the blocking `accept` so the thread can observe the stop flag.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(thread) = self.accept_thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve_connection(mut stream: TcpStream, handler: &Handler, requests: &Mutex<Vec<MockRequest>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Some(request) = read_request(&mut stream) else {
        return;
    };
    requests.lock().expect("request log").push(request.clone());
    let reply = handler(&request);
    if !reply.delay.is_zero() {
        std::thread::sleep(reply.delay);
    }
    if reply.hang_up {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    }
    let reason = match reply.status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        412 => "Precondition Failed",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    };
    let extra_headers: String = reply
        .headers
        .iter()
        .map(|(name, value)| format!("{}: {}\r\n", name, value))
        .collect();
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n",
        reply.status,
        reason,
        extra_headers,
        reply.body.len()
    );
    let body = match reply.cut_body_at {
        Some(cut) => &reply.body.as_bytes()[..cut.min(reply.body.len())],
        None => reply.body.as_bytes(),
    };
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    if reply.cut_body_at.is_some() && !reply.hold_open.is_zero() {
        let _ = stream.flush();
        std::thread::sleep(reply.hold_open);
    }
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Both);
}

fn read_request(stream: &mut TcpStream) -> Option<MockRequest> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
    };
    let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = head.lines();
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_string();
    let path = request_line.next()?.to_string();
    let content_length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body_end = (body_start + content_length).min(buffer.len());
    let body = String::from_utf8_lossy(&buffer[body_start..body_end]).to_string();
    Some(MockRequest { method, path, body })
}
