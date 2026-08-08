use crate::config::LoggingSettings;
use std::fs::{self, File};
use std::path::Path;
use std::sync::Once;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

static LOGGER_INIT: Once = Once::new();

/// Инициализирует глобальный логгер.
///
/// Приоритет источника пути к файлу:
/// 1. Аргумент `log_file`
/// 2. Переменная окружения `LOG_FILE`
/// 3. stdout (если ни один не задан)
///
/// Уровень логирования берётся из `LOG_LEVEL` (по умолчанию "info").
pub fn init_logger(settings: &LoggingSettings) {
    LOGGER_INIT.call_once(|| {
        let env_filter = EnvFilter::try_new(settings.level.clone()).unwrap();

        if settings.file_path != "" {
            // Создаём директорию, если нужно
            if let Some(parent) = Path::new(&settings.file_path).parent() {
                fs::create_dir_all(parent).expect("Не удалось создать директорию для логов");
            }
            // Лог в файл
            let file = File::create(&settings.file_path).expect("Не удалось создать файл лога");
            fmt()
                .with_writer(file)
                .with_env_filter(env_filter)
                .with_ansi(false)
                .init();
        } else {
            // Лог в stdout в компактном формате
            fmt().with_env_filter(env_filter).compact().init();
        }
        info!("Логгер инициализирован");
    });
}
