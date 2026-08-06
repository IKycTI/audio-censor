mod config;
mod logging;

use logging::init_logger;
use tracing::{error, info};

use config::{BanSet, Settings};

const CONFIG_PATH: &str = "config/default.toml";

fn main() {
    let settings = Settings::new(CONFIG_PATH);
    init_logger(settings.logging);

    let _word_dict = match BanSet::new(&settings.decision.words_dict_path) {
        Ok(dict) => {
            info!("Словарь загружен");
            dict
        }
        Err(e) => {
            error!("Ошибка при загрузке словаря: {}", e);
            return;
        }
    };

    info!("Запуск censor_ai");
}
