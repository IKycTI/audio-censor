use crate::config::LoggingSettings;
use std::fs::{self, File};
use std::path::Path;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

static LOGGER_INIT: Once = Once::new();
static LOGS_TO_FILE: AtomicBool = AtomicBool::new(false);

/// Пишется ли лог в файл (а не в терминал)
pub fn logs_to_file() -> bool {
    LOGS_TO_FILE.load(Ordering::Relaxed)
}

/// Инициализирует глобальный логгер по секции `[logging]` конфига:
/// уровень — `level`, вывод — в файл `file_path` или в stdout, если путь пуст.
/// Повторные вызовы ничего не делают.
pub fn init_logger(settings: &LoggingSettings) -> Result<(), String> {
    let mut result = Ok(());
    LOGGER_INIT.call_once(|| result = try_init_logger(settings));
    result
}

fn try_init_logger(settings: &LoggingSettings) -> Result<(), String> {
    let env_filter = EnvFilter::try_new(&settings.level)
        .map_err(|e| format!("неверный logging.level '{}': {}", settings.level, e))?;

    if !settings.file_path.is_empty() {
        // Создаём директорию, если нужно
        if let Some(parent) = Path::new(&settings.file_path).parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "не удалось создать директорию для логов {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }
        // Лог в файл
        let file = File::create(&settings.file_path)
            .map_err(|e| format!("не удалось создать файл лога {}: {}", settings.file_path, e))?;
        fmt()
            .with_writer(file)
            .with_env_filter(env_filter)
            .with_ansi(false)
            .init();
        LOGS_TO_FILE.store(true, Ordering::Relaxed);
    } else {
        // Лог в stdout в компактном формате
        fmt().with_env_filter(env_filter).compact().init();
    }
    info!("Логгер инициализирован");
    Ok(())
}
