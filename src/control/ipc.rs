use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::error;

use crate::control::ControlCommand;
use crate::metrics::Metrics;

#[derive(Clone)]
pub struct ControlHandle {
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    metrics: Arc<Metrics>,
    dict_reload_requested: Arc<AtomicBool>,
    metrics_file: String,
}

impl ControlHandle {
    pub fn new(
        running: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
        metrics: Arc<Metrics>,
        dict_reload_requested: Arc<AtomicBool>,
        metrics_file: String,
    ) -> Self {
        Self {
            running,
            paused,
            metrics,
            dict_reload_requested,
            metrics_file,
        }
    }

    pub fn execute(&self, cmd: ControlCommand) -> String {
        match cmd {
            ControlCommand::Status => {
                let snap = self.metrics.snapshot();
                serde_json::to_string(&snap).unwrap_or_else(|_| "ERROR: serialize".into())
            }
            ControlCommand::Pause => {
                self.paused.store(true, Ordering::Relaxed);
                "OK: paused".into()
            }
            ControlCommand::Resume => {
                self.paused.store(false, Ordering::Relaxed);
                "OK: resumed".into()
            }
            ControlCommand::ReloadDict => {
                self.dict_reload_requested.store(true, Ordering::Relaxed);
                "OK: reload requested".into()
            }
            ControlCommand::SaveMetrics => {
                // Счётчики атомарные, поэтому снимок можно сделать прямо из IPC-потока,
                // не дёргая оркестратор
                if self.metrics_file.is_empty() {
                    return "ERROR: metrics_file не задан в конфиге".into();
                }
                match self.metrics.save(&self.metrics_file) {
                    Ok(()) => format!("OK: metrics saved to {}", self.metrics_file),
                    Err(e) => format!("ERROR: {}", e),
                }
            }
            ControlCommand::Shutdown => {
                self.running.store(false, Ordering::Relaxed);
                "OK: shutting down".into()
            }
        }
    }
}
pub fn run_control_server(socket_path: &str, handle: ControlHandle) -> std::io::Result<()> {
    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let handle = handle.clone();
                std::thread::spawn(move || handle_connection(stream, handle));
            }
            Err(e) => error!("Ошибка приёма соединения на control-сокете: {}", e),
        }
    }
    Ok(())
}

fn handle_connection(stream: UnixStream, handle: ControlHandle) {
    let reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut writer = stream;

    for line in reader.lines() {
        let Ok(line) = line else { break };
        let response = match ControlCommand::parse(&line) {
            Ok(cmd) => handle.execute(cmd),
            Err(e) => format!("ERROR: {}", e),
        };
        let _ = writeln!(writer, "{}", response);
    }
}
