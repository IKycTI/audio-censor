use cpal::{
    Device,
    traits::{DeviceTrait, HostTrait},
};
use cpal::{StreamConfig, SupportedStreamConfig};

use crate::config::AudioSettings;
use tracing::info;

pub fn get_device(output_device: &str) -> Result<Device, String> {
    let host = cpal::default_host();

    let device = if output_device.is_empty() || output_device.to_lowercase() == "default" {
        host.default_output_device()
    } else {
        host.output_devices().map_err(|e| e.to_string())?.find(|d| {
            d.description()
                .is_ok_and(|desc| desc.name() == output_device)
        })
    };
    let device = device.ok_or_else(|| {
        format!(
            "устройство вывода '{}' не найдено (список устройств: --show-device output)",
            output_device
        )
    })?;

    match device.description() {
        Ok(desc) => info!("Используем устройство вывода: {}", desc.name()),
        Err(_) => info!("Используем устройство вывода (описание недоступно)"),
    }
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
    let output_config = selected_config.ok_or_else(|| {
        format!(
            "устройство не поддерживает формат {:?}, каналов: {}, частоту {} Гц \
             (параметры устройств: --show-device output)",
            audio_config.sample_format, audio_config.channels, audio_config.sample_rate
        )
    })?;

    info!("Найдена конфигурация вывода: {:?}", output_config);
    Ok(output_config.config())
}

pub fn show_device() {
    let host = cpal::default_host();

    println!("==================================================");
    println!(" Устройства вывода звука (колонки, наушники)");
    println!("==================================================");

    // Устройства по умолчанию может не быть — это не повод падать при выводе списка
    let default_output_desc = host
        .default_output_device()
        .and_then(|device| device.description().ok());

    let devices = match host.output_devices() {
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

        let is_default = default_output_desc.as_ref() == Some(&desc);
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
