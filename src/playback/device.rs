use cpal::StreamConfig;
use cpal::{
    traits::{DeviceTrait, HostTrait},
    Device,
};

use crate::config::AudioSettings;
use tracing::{info, warn};

pub fn get_device(output_device: &str) -> Result<Device, String> {
    let host = cpal::default_host();

    let device: Option<Device>;
    if output_device.is_empty() || output_device.to_lowercase() == "default" {
        device = host.default_output_device();
    } else {
        let devices = host.output_devices().map_err(|e| e.to_string())?;

        device = devices
            .filter_map(|d| {
                let description = d.description().ok()?;
                if description.name() == output_device {
                    Some(d)
                } else {
                    None
                }
            })
            .next();
    }
    let device = device.expect("Устройство вывода не найдено");

    let device_desc = device.description().ok().unwrap();
    info!("Используем устройство вывода: {}", device_desc.name());
    Ok(device)
}

pub fn get_config(device: &Device, audio_config: &AudioSettings) -> Result<StreamConfig, String> {
    // Получаем конфиг вывода (используем его как основу)
    let output_configs = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?;
    let mut selected_config = None;

    for config in output_configs {
        if config.sample_format() == audio_config.sample_format
            && config.channels() == audio_config.channels
            && config.min_sample_rate() <= audio_config.sample_rate
            && config.max_sample_rate() >= audio_config.sample_rate
        {
            selected_config = Some(config.with_max_sample_rate());
            break;
        }
    }
    let output_config = selected_config
        .or_else(|| {
            warn!(
                "Не удалось найти подходящий конфигурация вывода, используем конфиг по умолчанию"
            );
            device.default_output_config().ok()
        })
        .ok_or("Не удалось найти подходящий конфигурация вывода".to_string())?;

    info!("Найдена конфигурация вывода: {:?}", output_config);
    Ok(output_config.config())
}

pub fn show_device() {
    let host = cpal::default_host();

    println!("==================================================");
    println!(" Устройства вывода звука (колонки, наушники)");
    println!("==================================================");

    let default_output_desc = host
        .default_output_device()
        .and_then(|device| device.description().ok())
        .unwrap();

    let devices = match host.output_devices() {
        Ok(devices) => devices,
        Err(err) => {
            println!("  Ошибка получения списка устройств: {err}");
            return;
        }
    };

    let mut count = 0;

    for device in devices {
        let desc = device.description().ok().unwrap();

        let is_default = default_output_desc == desc;
        let marker = if is_default { "*" } else { " " };

        println!("{marker} [{count:2}] {desc}");

        if let Ok(config) = device.default_output_config() {
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
        println!("  Устройства вывода не найдены");
    } else {
        println!("--------------------------------------------------");
        println!("  Всего устройств вывода: {count}");
        println!("  * — устройство по умолчанию");
    }
}
