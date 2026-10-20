//! Control-сокет демона. Через `interprocess`: на Unix это Unix-сокет (файл), на Windows —
//! именованный канал (`\\.\pipe\...`). Адрес в обоих случаях — путь, `PathBuf`.

use std::io::{self, BufRead, BufReader, ErrorKind, Write};
use std::path::{Path, PathBuf};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{GenericFilePath, Listener, ListenerOptions, Name, Stream};
use tracing::{error, info, warn};

use crate::control::ControlCommand;
use crate::metrics::{Metrics, MetricsReporter};

#[derive(Clone)]
pub struct ControlHandle {
    running: Arc<AtomicBool>,
    metrics: Arc<Metrics>,
    dict_reload_requested: Arc<AtomicBool>,
    reporter: Option<MetricsReporter>,
}

impl ControlHandle {
    pub fn new(
        running: Arc<AtomicBool>,
        metrics: Arc<Metrics>,
        dict_reload_requested: Arc<AtomicBool>,
        reporter: Option<MetricsReporter>,
    ) -> Self {
        Self {
            running,
            metrics,
            dict_reload_requested,
            reporter,
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
                let Some(reporter) = &self.reporter else {
                    return "ERROR: сохранение метрик выключено (metrics.enabled / metrics.dir)"
                        .into();
                };
                // Промежуточный снимок: в sessions.jsonl не попадает, итог перезапишет файл
                match reporter.save(&self.metrics, false) {
                    Ok(path) => format!("OK: metrics saved to {}", path.display()),
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
    listener: Listener,
    file: SocketFile,
}

/// Файл сокета, который удаляется вместе с этим объектом. На Windows файла нет:
/// именованный канал исчезает сам, когда закрывается последний его дескриптор
pub struct SocketFile {
    #[cfg_attr(not(unix), allow(dead_code))]
    path: PathBuf,
}

impl Drop for SocketFile {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Err(e) = std::fs::remove_file(&self.path)
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
        match connect(path) {
            Ok(_) => {
                return Err(format!(
                    "демон уже запущен: на {} отвечает другой экземпляр",
                    path.display()
                ));
            }
            // Файл есть, но никто не слушает — остался от аварийно завершённого демона.
            // Только Unix: канал Windows без сервера не существует, там будет NotFound
            Err(e) if e.kind() == ErrorKind::ConnectionRefused => {
                warn!("Удаляю устаревший control-сокет {}", path.display());
                std::fs::remove_file(path).map_err(|e| {
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

        let listener = name(path)
            .and_then(|name| {
                ListenerOptions::new()
                    .name(name)
                    // Файл удаляет SocketFile, а не listener: он живёт в потоке приёма до конца
                    // процесса и сам не уничтожается
                    .reclaim_name(false)
                    .create_sync()
            })
            .map_err(|e| format!("не удалось создать сокет {}: {}", path.display(), e))?;
        // С этого момента файл наш и удалится при любой ошибке запуска
        let file = SocketFile {
            path: path.to_path_buf(),
        };

        // Команды управления (в том числе shutdown) — только владельцу процесса.
        // На Windows этим ведает ACL канала по умолчанию: полный доступ у владельца, SYSTEM
        // и администраторов, остальным только чтение — отправить команду они не могут
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("не удалось выставить права на {}: {}", path.display(), e))?;
        }

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

/// Подключиться к control-сокету по пути (клиент управления и проверка «демон уже запущен»)
pub fn connect(path: &Path) -> io::Result<Stream> {
    Stream::connect(name(path)?)
}

fn name(path: &Path) -> io::Result<Name<'_>> {
    path.to_fs_name::<GenericFilePath>()
}

fn accept_loop(listener: Listener, handle: ControlHandle) {
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

fn handle_connection(stream: Stream, handle: ControlHandle) {
    // Чтение и запись — через `&Stream`, клонировать соединение не нужно
    let reader = BufReader::new(&stream);
    let mut writer = &stream;

    for line in reader.lines() {
        let Ok(line) = line else { break };
        let response = match ControlCommand::parse(&line) {
            Ok(cmd) => handle.execute(cmd),
            Err(e) => format!("ERROR: {}", e),
        };
        let _ = writeln!(writer, "{}", response);
    }
}
