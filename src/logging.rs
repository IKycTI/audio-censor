use crate::{config::LoggingSettings, session::Session};
use std::fs::{self, File};
use std::path::Path;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, fmt};

const LATEST_LINK: &str = "latest.log";
#[cfg(windows)]
const LATEST_POINTER: &str = "latest.txt";

static LOGGER_INIT: Once = Once::new();
static LOGS_TO_FILE: AtomicBool = AtomicBool::new(false);

/// Пишется ли лог в файл (а не в терминал)
pub fn logs_to_file() -> bool {
    LOGS_TO_FILE.load(Ordering::Relaxed)
}

/// Инициализирует глобальный логгер по секции `[logging]` конфига:
/// уровень — `level`, вывод — в файл `<dir>/<сеанс>.log` или в stdout, если `dir` пуст.
/// Повторные вызовы ничего не делают.
pub fn init_logger(settings: &LoggingSettings, session: &Session) -> Result<(), String> {
    let mut result = Ok(());
    LOGGER_INIT.call_once(|| result = try_init_logger(settings, session));
    result
}

fn try_init_logger(settings: &LoggingSettings, session: &Session) -> Result<(), String> {
    let env_filter = EnvFilter::try_new(&settings.level)
        .map_err(|e| format!("неверный logging.level '{}': {}", settings.level, e))?;

    if !settings.dir.is_empty() {
        let dir = Path::new(&settings.dir);
        fs::create_dir_all(dir).map_err(|e| {
            format!(
                "не удалось создать директорию для логов {}: {}",
                dir.display(),
                e
            )
        })?;
        // Файл на сеанс: прошлые запуски не затираются
        let file_name = format!("{}.log", session.id);
        let path = dir.join(&file_name);
        let file = File::create(&path)
            .map_err(|e| format!("не удалось создать файл лога {}: {}", path.display(), e))?;
        let latest = link_latest(dir, &file_name);
        fmt()
            .with_writer(file)
            .with_env_filter(env_filter)
            .with_ansi(false)
            .init();
        LOGS_TO_FILE.store(true, Ordering::Relaxed);
        // Логгер уже работает — о неудаче со ссылкой можно сообщить через него
        if let Err(e) = latest {
            warn!(
                "Не удалось обновить {}: {}",
                dir.join(LATEST_LINK).display(),
                e
            );
        }
    } else {
        // Лог в stdout в компактном формате
        fmt().with_env_filter(env_filter).compact().init();
    }
    info!("Логгер инициализирован");
    Ok(())
}

/// `latest.log` → лог текущего сеанса, для `tail -f`. Ссылка относительная —
/// не ломается при переносе каталога
fn link_latest(dir: &Path, file_name: &str) -> std::io::Result<()> {
    let link = dir.join(LATEST_LINK);
    match fs::remove_file(&link) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    symlink(file_name, &link, dir)
}

#[cfg(unix)]
fn symlink(file_name: &str, link: &Path, _dir: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(file_name, link)
}

/// Символическая ссылка на Windows требует прав администратора или режима разработчика.
/// Без них вместо ссылки — `latest.txt` с именем файла лога текущего сеанса
#[cfg(windows)]
fn symlink(file_name: &str, link: &Path, dir: &Path) -> std::io::Result<()> {
    let pointer = dir.join(LATEST_POINTER);
    match std::os::windows::fs::symlink_file(file_name, link) {
        Ok(()) => match fs::remove_file(&pointer) {
            // Остался от запуска без прав на ссылки — иначе он указывал бы на старый лог
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
        Err(_) => fs::write(pointer, file_name),
    }
}
