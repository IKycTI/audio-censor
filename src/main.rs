mod logging;

use dotenvy::dotenv;
use logging::init_logger;
use tracing::info;

fn main() {
    dotenv().ok();

    let log_file = std::env::var("LOG_FILE")
        .ok()
        .filter(|r| !r.trim().is_empty());
    init_logger(log_file.as_deref());

    info!("Запуск censor_ai");
}
