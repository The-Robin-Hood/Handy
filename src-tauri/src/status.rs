use log::{debug, error};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
};
use tauri::{AppHandle, Manager};

const STATUS_PORT: u16 = 9842;

// ── State ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum ActivityStatus {
    Idle,
    Recording,
    Transcribing,
    Processing,
}

impl ActivityStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActivityStatus::Idle => "idle",
            ActivityStatus::Recording => "recording",
            ActivityStatus::Transcribing => "transcribing",
            ActivityStatus::Processing => "processing",
        }
    }
}

// ── Manager ──────────────────────────────────────────────────────────────────

pub struct StatusManager {
    current: Arc<Mutex<ActivityStatus>>,
}

impl StatusManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            current: Arc::new(Mutex::new(ActivityStatus::Idle)),
        })
    }

    pub fn set_status(&self, status: ActivityStatus) {
        let mut current = self.current.lock().unwrap();
        if *current != status {
            *current = status.clone();
            drop(current);
            debug!("Activity status changed to: {}", status.as_str());
        }
    }

    pub fn get_status(&self) -> ActivityStatus {
        self.current.lock().unwrap().clone()
    }
}

pub fn set_status_safe(app: &AppHandle, status: ActivityStatus) {
    if let Some(sm) = app.try_state::<Arc<StatusManager>>() {
        sm.set_status(status);
    }
}

// ── HTTP Server ───────────────────────────────────────────────────────────────

pub fn start_server(manager: Arc<StatusManager>) {
    thread::spawn(move || {
        let addr = format!("127.0.0.1:{}", STATUS_PORT);
        let listener = match TcpListener::bind(&addr) {
            Ok(l) => {
                debug!("Handy status server listening on http://{}", addr);
                l
            }
            Err(e) => {
                error!("Failed to bind status server on {}: {}", addr, e);
                return;
            }
        };

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let manager = Arc::clone(&manager);
                    thread::spawn(move || handle_request(stream, manager));
                }
                Err(e) => error!("Status server connection error: {}", e),
            }
        }
    });
}

fn handle_request(mut stream: TcpStream, manager: Arc<StatusManager>) {
    let mut buf = [0u8; 512];
    if stream.read(&mut buf).is_err() {
        return;
    }

    let request = String::from_utf8_lossy(&buf);
    let first_line = request.lines().next().unwrap_or("");

    if !first_line.starts_with("GET /status ") {
        let _ = stream.write_all(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\n\r\nNot Found",
        );
        return;
    }

    let body = manager.get_status().as_str();
    let response = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/plain\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         \r\n\
         {}",
        body.len(),
        body
    );

    let _ = stream.write_all(response.as_bytes());
}
