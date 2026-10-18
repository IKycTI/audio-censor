use cpal::{
    Device,
    traits::{DeviceTrait, HostTrait},
};
use cpal::{StreamConfig, SupportedStreamConfig};

use crate::config::AudioSettings;
use tracing::info;

pub fn get_device(input_device: &str) -> Result<Device, String> {
    let host = cpal::default_host();

    let device = if input_device.is_empty() || input_device.to_lowercase() == "default" {
        host.default_input_device()
    } else {
        host.input_devices().map_err(|e| e.to_string())?.find(|d| {
            d.description()
                .is_ok_and(|desc| desc.name() == input_device)
        })
    };
    let device = device.ok_or_else(|| {
        format!(
            "устройство ввода '{}' не найдено (список устройств: --show-device input)",
            input_device
        )
    })?;

    match device.description() {
        Ok(desc) => info!("Используем устройство ввода: {}", desc.name()),
        Err(_) => info!("Используем устройство ввода (описание недоступно)"),
    }
    Ok(device)
}

pub fn get_config(device: &Device, audio_config: &AudioSettings) -> Result<StreamConfig, String> {
    // Получаем конфиг входа (используем его как основу)
    let input_configs = device
        .supported_input_configs()
        .map_err(|e| e.to_string())?;
    let mut selected_config = None;

    for config in input_configs {
        if config.sample_format() == audio_config.sample_format
            && config.channels() == audio_config.channels
            && config.min_sample_rate() <= audio_config.sample_rate
            && config.max_sample_rate() >= audio_config.sample_rate
        {
            selected_config = Some(SupportedStreamConfig::new(
                config.channels(),
                audio_config.sample_rate,
                *config.buffer_size(),
                config.sample_format(),
            ));
            break;
        }
    }
    // Без запасного варианта: конфигурация по умолчанию не совпадёт с форматом T,
    // числом каналов и частотой, под которые собран весь конвейер
    let input_config = selected_config.ok_or_else(|| {
        format!(
            "устройство не поддерживает формат {:?}, каналов: {}, частоту {} Гц \
             (параметры устройств: --show-device input)",
            audio_config.sample_format, audio_config.channels, audio_config.sample_rate
        )
    })?;

    info!("Найдена конфигурация ввода: {:?}", input_config);
    Ok(input_config.config())
}

pub fn show_device() {
    let host = cpal::default_host();

    println!("==================================================");
    println!(" Устройства ввода звука (микрофоны)");
    println!("==================================================");

    // Устройства по умолчанию может не быть — это не повод падать при выводе списка
    let default_input_desc = host
        .default_input_device()
        .and_then(|device| device.description().ok());

    let devices = match host.input_devices() {
        Ok(devices) => devices,
        Err(err) => {
            println!("  Ошибка получения списка устройств: {err}");
            return;
        }
    };

    let mut count = 0;

    for device in devices {
        let Ok(desc) = device.description() else {
            println!("  [{count:2}] <описание недоступно>");
            count += 1;
            continue;
        };

        let is_default = default_input_desc.as_ref() == Some(&desc);
        let marker = if is_default { "*" } else { " " };

        println!("{marker} [{count:2}] {desc}");

        if let Ok(config) = device.default_input_config() {
            println!(
                "\tканалы: {:>2}, частота: {:>6} Гц, формат: {:?}",
                config.channels(),
                config.sample_rate(),
                config.sample_format()
            );
        } else {
            println!("\tпараметры по умолчанию недоступны");
        }

        count += 1;
    }

    if count == 0 {
        println!("  Устройства ввода не найдены");
    } else {
        println!("--------------------------------------------------");
        println!("  Всего устройств ввода: {count}");
        println!("  * — устройство по умолчанию");
    }
}
