use std::fs::{self, File};
use std::path::Path;
use std::sync::Once;
use tracing_subscriber::{EnvFilter, fmt};

static LOGGER_INIT: Once = Once::new();

/// Инициализирует глобальный логгер.
///
/// Приоритет источника пути к файлу:
/// 1. Аргумент `log_file`
/// 2. Переменная окружения `LOG_FILE`
/// 3. stdout (если ни один не задан)
///
/// Уровень логирования берётся из `LOG_LEVEL` (по умолчанию "info").
pub fn init_logger(log_file: Option<&str>) {
    LOGGER_INIT.call_once(|| {
        // Определяем уровень логирования
        let log_level = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "debug".to_string());

        let env_filter = EnvFilter::try_new(log_level).unwrap();

        match log_file {
            Some(path) => {
                // Создаём директорию, если нужно
                if let Some(parent) = Path::new(path).parent() {
                    fs::create_dir_all(parent).expect("Не удалось создать директорию для логов");
                }

                let file = File::create(path).expect("Не удалось создать файл лога");
                fmt()
                    .with_writer(file)
                    .with_env_filter(env_filter)
                    .with_ansi(false)
                    .init();
            }
            None => {
                // Лог в stdout в компактном формате
                fmt().with_env_filter(env_filter).compact().init();
            }
        }
    });
}
