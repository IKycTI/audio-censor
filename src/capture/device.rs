use cpal::{
    Device,
    traits::{DeviceTrait, HostTrait},
};
use cpal::{StreamConfig, SupportedStreamConfig};

use crate::config::AudioSettings;
use tracing::{info, warn};

pub fn get_device(input_device: &str) -> Result<Device, String> {
    let host = cpal::default_host();

    let device: Option<Device>;
    if input_device.is_empty() || input_device.to_lowercase() == "default" {
        device = host.default_input_device();
    } else {
        let devices = host.input_devices().map_err(|e| e.to_string())?;

        device = devices
            .filter_map(|d| {
                let description = d.description().ok()?;
                if description.name() == input_device {
                    Some(d)
                } else {
                    None
                }
            })
            .next();
    }
    let device = device.expect("Устройство ввода не найдено");

    let device_desc = device.description().ok().unwrap();
    info!("Используем устройство ввода: {}", device_desc.name());
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
    let input_config = selected_config
        .or_else(|| {
            warn!("Не удалось найти подходящий конфигурация ввода, используем конфигурацию по умолчанию");
            device.default_input_config().ok()
        })
        .ok_or("Не удалось найти подходящий конфигурация ввода".to_string())?;

    info!("Найдена конфигурация ввода: {:?}", input_config);
    Ok(input_config.config())
}

pub fn show_device() {
    let host = cpal::default_host();

    println!("==================================================");
    println!(" Устройства ввода звука (микрофоны)");
    println!("==================================================");

    let default_input_desc = host
        .default_input_device()
        .and_then(|device| device.description().ok())
        .unwrap();

    let devices = match host.input_devices() {
        Ok(devices) => devices,
        Err(err) => {
            println!("  Ошибка получения списка устройств: {err}");
            return;
        }
    };

    let mut count = 0;

    for device in devices {
        let desc = device.description().ok().unwrap();

        let is_default = default_input_desc == desc;
        let marker = if is_default { "*" } else { " " };

        println!("{marker} [{count:2}] {desc}");

        if let Ok(config) = device.default_input_config() {
            println!(
                "\tканалы: {:>2}, частота: {:>6} Hz, формат: {:?}",
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
