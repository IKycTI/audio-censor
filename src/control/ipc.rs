use std::fs::{self, Permissions};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::{error, info, warn};

use crate::control::ControlCommand;
use crate::metrics::Metrics;

#[derive(Clone)]
pub struct ControlHandle {
    running: Arc<AtomicBool>,
    metrics: Arc<Metrics>,
    dict_reload_requested: Arc<AtomicBool>,
    metrics_file: String,
}

impl ControlHandle {
    pub fn new(
        running: Arc<AtomicBool>,
        metrics: Arc<Metrics>,
        dict_reload_requested: Arc<AtomicBool>,
        metrics_file: String,
    ) -> Self {
        Self {
            running,
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
/// Занятый control-сокет. Занимается в самом начале запуска — чтобы второй экземпляр
/// демона узнал о первом до загрузки модели, — а команды обслуживает после `serve`
pub struct ControlSocket {
    listener: UnixListener,
    file: SocketFile,
}

/// Файл сокета, который удаляется вместе с этим объектом
pub struct SocketFile {
    path: PathBuf,
}

impl Drop for SocketFile {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(&self.path)
            && e.kind() != ErrorKind::NotFound
        {
            error!(
                "Не удалось удалить control-сокет {}: {}",
                self.path.display(),
                e
            );
        }
    }
}

impl ControlSocket {
    /// Занять сокет. Если на нём уже отвечает другой демон — ошибка, а не перехват:
    /// иначе второй экземпляр молча отобрал бы управление у первого
    pub fn bind(path: &Path) -> Result<Self, String> {
        match UnixStream::connect(path) {
            Ok(_) => {
                return Err(format!(
                    "демон уже запущен: на {} отвечает другой экземпляр",
                    path.display()
                ));
            }
            // Файл есть, но никто не слушает — остался от аварийно завершённого демона
            Err(e) if e.kind() == ErrorKind::ConnectionRefused => {
                warn!("Удаляю устаревший control-сокет {}", path.display());
                fs::remove_file(path).map_err(|e| {
                    format!(
                        "не удалось удалить устаревший сокет {}: {}",
                        path.display(),
                        e
                    )
                })?;
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(format!("не удалось проверить {}: {}", path.display(), e)),
        }

        let listener = UnixListener::bind(path)
            .map_err(|e| format!("не удалось создать сокет {}: {}", path.display(), e))?;
        // С этого момента файл наш и удалится при любой ошибке запуска
        let file = SocketFile {
            path: path.to_path_buf(),
        };

        // Команды управления (в том числе shutdown) — только владельцу процесса
        fs::set_permissions(path, Permissions::from_mode(0o600))
            .map_err(|e| format!("не удалось выставить права на {}: {}", path.display(), e))?;

        info!("Control-сокет: {}", path.display());
        Ok(ControlSocket { listener, file })
    }

    /// Начать обслуживать команды в отдельном потоке. Возвращённый `SocketFile` нужно
    /// держать до завершения демона: при его уничтожении файл сокета удаляется
    pub fn serve(self, handle: ControlHandle) -> SocketFile {
        let ControlSocket { listener, file } = self;
        std::thread::spawn(move || accept_loop(listener, handle));
        file
    }
}

fn accept_loop(listener: UnixListener, handle: ControlHandle) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let handle = handle.clone();
                std::thread::spawn(move || handle_connection(stream, handle));
            }
            Err(e) => error!("Ошибка приёма соединения на control-сокете: {}", e),
        }
    }
}

fn handle_connection(stream: UnixStream, handle: ControlHandle) {
    let reader = match stream.try_clone() {
        Ok(s) => BufReader::new(s),
        Err(e) => {
            error!("Не удалось подготовить control-соединение: {}", e);
            return;
        }
    };
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
