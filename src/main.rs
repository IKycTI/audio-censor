use audio_censor::buffer;
use audio_censor::capture;
use audio_censor::config::{BanSet, Settings};
use audio_censor::logging::init_logger;
use audio_censor::playback;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use cpal::traits::StreamTrait;
use cpal::SizedSample;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "audio-censor", version, about = "", propagate_version = true)]
struct Cli {
    /// Путь к файлу конфигурации
    #[arg(short = 'c', long, value_name = "PATH")]
    config_path: Option<PathBuf>,

    /// Показать устройства вместо запуска start
    #[arg(short, long, value_enum, value_name = "FILTER")]
    show_device: Option<DeviceFilter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DeviceFilter {
    Input,
    Output,
    All,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.show_device {
        Some(filter) => Ok(show_device(filter)),
        // Если аргументы вообще не переданы — запускаем start
        None => {
            // let settings = cli.config.map(|path| Settings::new(path.to_str())).unwrap();
            let settings = Settings::new(cli.config_path);

            init_logger(&settings.logging);
            match settings.audio.sample_format {
                cpal::SampleFormat::I8 => start::<i8>(settings),
                cpal::SampleFormat::I16 => start::<i16>(settings),
                cpal::SampleFormat::I32 => start::<i32>(settings),
                cpal::SampleFormat::I64 => start::<i64>(settings),
                cpal::SampleFormat::U8 => start::<u8>(settings),
                cpal::SampleFormat::U16 => start::<u16>(settings),
                cpal::SampleFormat::U32 => start::<u32>(settings),
                cpal::SampleFormat::U64 => start::<u64>(settings),
                cpal::SampleFormat::F32 => start::<f32>(settings),
                cpal::SampleFormat::F64 => start::<f64>(settings),
                _ => Err(anyhow::anyhow!("Не поддерживаемы формат сэмпла.")),
            }
        }
    }
}

fn start<T: SizedSample + Default + Send + 'static>(settings: Settings) -> Result<()> {
    let _word_dict = match BanSet::new(&settings.decision.words_dict_path) {
        Ok(dict) => {
            info!("Словарь загружен");
            dict
        }
        Err(e) => {
            error!("Ошибка при загрузке словаря: {}", e);
            return Err(anyhow::anyhow!(e));
        }
    };

    // Временный кусок для теста уйдет в pipeline.rs
    // ======================================================
    let (prod, cons) = buffer::create_scsp::<T>(settings.buffer.ring_capacity_ms);

    let input_device = match capture::get_device(&settings.audio.input_device) {
        Ok(device) => device,
        Err(e) => {
            error!("Ошибка при получении устройства ввода: {}", e);
            return Err(anyhow::anyhow!(e));
        }
    };

    let input_config = match capture::get_config(&input_device, &settings.audio) {
        Ok(config) => config,
        Err(e) => {
            error!("Ошибка при получении конфигурации устройства ввода: {}", e);
            return Err(anyhow::anyhow!(e));
        }
    };

    let input_stream = match capture::get_stream(input_device, input_config, prod) {
        Ok(stream) => stream,
        Err(e) => {
            error!("Ошибка при получении потока устройства ввода: {}", e);
            return Err(anyhow::anyhow!(e));
        }
    };

    let output_device = match playback::get_device(&settings.audio.output_device) {
        Ok(device) => device,
        Err(e) => {
            error!("Ошибка при получении устройства вывода: {}", e);
            return Err(anyhow::anyhow!(e));
        }
    };

    let output_config = match playback::get_config(&output_device, &settings.audio) {
        Ok(config) => config,
        Err(e) => {
            error!("Ошибка при получении конфигурации устройства ввода: {}", e);
            return Err(anyhow::anyhow!(e));
        }
    };

    let output_stream = match playback::get_stream(output_device, output_config, cons) {
        Ok(stream) => stream,
        Err(e) => {
            error!("Ошибка при получении потока устройства ввода: {}", e);
            return Err(anyhow::anyhow!(e));
        }
    };

    input_stream.play()?;
    output_stream.play()?;

    info!("Запуск censor_ai");
    println!("\n=== МОНИТОРИНГ МИКРОФОНА ===");
    println!("Всё, что вы говорите, сразу воспроизводится в динамиках");
    println!("Нажмите Ctrl+C для выхода\n");

    // Бесконечный цикл с обработкой сигналов
    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        eprintln!("\nОстанавливаем потоки...");
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    })?;

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(100));
    }

    // Останавливаем потоки
    drop(input_stream);
    drop(output_stream);

    info!("Завершено");

    // ======================================================

    Ok(())
}

fn show_device(filter: DeviceFilter) {
    match filter {
        DeviceFilter::Input => capture::show_device(),
        DeviceFilter::Output => playback::show_device(),
        DeviceFilter::All => {
            capture::show_device();
            println!();
            playback::show_device();
        }
    }
}
