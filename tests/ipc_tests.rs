use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use audio_censor::config::ControlSettings;
use audio_censor::control::{ControlHandle, ControlSocket, connect};
use audio_censor::metrics::Metrics;

/// Отдельный каталог на каждый тест (тесты идут параллельно), удаляется по завершении
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("audio-censor-ipc-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    /// Unix — файл сокета в каталоге теста
    #[cfg(unix)]
    fn socket(&self) -> PathBuf {
        self.0.join("s.sock")
    }

    /// Windows — именованный канал: каталог ему не нужен, уникальность даёт имя каталога
    #[cfg(windows)]
    fn socket(&self) -> PathBuf {
        let name = self.0.file_name().unwrap().to_string_lossy();
        PathBuf::from(format!(r"\\.\pipe\{name}"))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn handle(running: Arc<AtomicBool>) -> ControlHandle {
    ControlHandle::new(
        running,
        Arc::new(Metrics::default()),
        Arc::new(AtomicBool::new(false)),
        None,
    )
}

fn send(path: &PathBuf, command: &str) -> String {
    let mut stream = connect(path).unwrap();
    writeln!(stream, "{}", command).unwrap();
    let mut response = String::new();
    BufReader::new(stream).read_line(&mut response).unwrap();
    response.trim().to_string()
}

// ── Занятие сокета ─────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn bind_creates_socket_with_owner_only_permissions() {
    let dir = TempDir::new("perm");
    let path = dir.socket();
    let _socket = ControlSocket::bind(&path).unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600, "права {:o}", mode & 0o777);
}

#[test]
fn bind_refuses_socket_of_running_daemon() {
    let dir = TempDir::new("live");
    let path = dir.socket();
    // Живой «первый демон»: сокет занят, хотя команды ещё не обслуживаются (грузится модель)
    let _first = ControlSocket::bind(&path).unwrap();

    let err = ControlSocket::bind(&path)
        .err()
        .expect("второй экземпляр должен получить ошибку");
    assert!(err.contains("уже запущен"), "{err}");
    #[cfg(unix)]
    assert!(path.exists(), "сокет первого демона нельзя удалять");
}

#[cfg(unix)]
#[test]
fn bind_replaces_stale_socket() {
    let dir = TempDir::new("stale");
    let path = dir.socket();
    // Демон упал: файл остался, но никто не слушает
    drop(UnixListener::bind(&path).unwrap());
    assert!(path.exists());

    let socket = ControlSocket::bind(&path);
    assert!(socket.is_ok(), "{:?}", socket.err());
}

#[cfg(unix)]
#[test]
fn bind_reports_missing_directory() {
    let dir = TempDir::new("nodir");
    let path = dir.0.join("нет").join("s.sock");
    let err = ControlSocket::bind(&path).err().unwrap();
    assert!(err.contains("не удалось создать сокет"), "{err}");
}

// ── Удаление файла (только Unix: канал Windows исчезает сам) ──────

#[cfg(unix)]
#[test]
fn socket_file_is_removed_if_never_served() {
    // Ошибка запуска после занятия сокета: файл не должен остаться
    let dir = TempDir::new("unserved");
    let path = dir.socket();
    drop(ControlSocket::bind(&path).unwrap());
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn socket_file_is_removed_after_serving() {
    let dir = TempDir::new("served");
    let path = dir.socket();
    let socket = ControlSocket::bind(&path).unwrap();
    let file = socket.serve(handle(Arc::new(AtomicBool::new(true))));
    assert!(path.exists());

    drop(file);
    assert!(!path.exists());
}

// ── Обслуживание команд ────────────────────────────────────────────

#[test]
fn served_socket_answers_commands() {
    let dir = TempDir::new("serve");
    let path = dir.socket();
    let running = Arc::new(AtomicBool::new(true));
    let _file = ControlSocket::bind(&path)
        .unwrap()
        .serve(handle(Arc::clone(&running)));

    assert!(send(&path, "status").contains("late_intervals"));
    assert!(send(&path, "неизвестно").starts_with("ERROR"));

    assert_eq!(send(&path, "shutdown"), "OK: shutting down");
    assert!(!running.load(Ordering::Relaxed));
}

#[test]
fn second_instance_is_refused_while_first_serves() {
    let dir = TempDir::new("second");
    let path = dir.socket();
    let _file = ControlSocket::bind(&path)
        .unwrap()
        .serve(handle(Arc::new(AtomicBool::new(true))));

    assert!(ControlSocket::bind(&path).is_err());
    // Первый продолжает работать
    assert!(send(&path, "status").contains("late_intervals"));
}

// ── Путь по умолчанию ──────────────────────────────────────────────

#[test]
fn explicit_socket_path_is_used_as_is() {
    let settings = ControlSettings {
        enabled: true,
        socket_path: "/some/where.sock".to_string(),
    };
    assert_eq!(
        settings.resolved_socket_path().unwrap(),
        PathBuf::from("/some/where.sock")
    );
}

#[cfg(unix)]
#[test]
fn empty_socket_path_goes_to_xdg_runtime_dir() {
    let settings = ControlSettings {
        enabled: true,
        socket_path: String::new(),
    };
    // Переменную окружения не меняем (тесты параллельны), проверяем по текущему значению
    match std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
        Some(dir) => assert_eq!(
            settings.resolved_socket_path().unwrap(),
            PathBuf::from(dir).join("audio-censor.sock")
        ),
        None => assert!(settings.resolved_socket_path().is_err()),
    }
}

#[cfg(windows)]
#[test]
fn empty_socket_path_goes_to_user_pipe() {
    let settings = ControlSettings {
        enabled: true,
        socket_path: String::new(),
    };
    let path = settings.resolved_socket_path().unwrap();
    assert!(
        path.to_string_lossy().starts_with(r"\\.\pipe\audio-censor"),
        "{}",
        path.display()
    );
}
