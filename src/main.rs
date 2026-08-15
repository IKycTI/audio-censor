use audio_censor::config::Settings;
use audio_censor::logging::init_logger;
use audio_censor::pipeline::{Orchestrator, OrchestratorError};
use audio_censor::{capture, playback};

use clap::{Parser, ValueEnum};
use cpal::SizedSample;
use dasp::{sample::Duplex, Sample};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.show_device {
        Some(filter) => {
            show_device(filter);
            ExitCode::SUCCESS
        }
        // Если аргументы вообще не переданы — запускаем start
        None => {
            let settings = match Settings::new(cli.config_path) {
                Ok(s) => s,
                Err(err) => {
                    error!("Ошибка получения конфигурации: {}", err);
                    return ExitCode::FAILURE;
                }
            };

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
                other => {
                    error!("Не поддерживаемый формат сэмпла: {other}.");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

fn start<T>(settings: Settings) -> ExitCode
where
    T: Default + Copy + Sample + Duplex<f32> + SizedSample + Send + 'static,
{
    info!("Запуск audio-censor");

    let mut orchestrator = match Orchestrator::<T>::new(settings) {
        Ok(o) => o,
        Err(err) => {
            match err {
                OrchestratorError::WordDictError(e) => error!("Ошибка при загрузке словаря: {}", e),
                OrchestratorError::CaptureDeviceError(e) => {
                    error!("Ошибка при получении устройства ввода: {}", e)
                }
                OrchestratorError::CaptureConfigError(e) => {
                    error!("Ошибка при получении конфигурации устройства ввода: {}", e)
                }
                OrchestratorError::CaptureStreamError(e) => {
                    error!("Ошибка при получении потока устройства ввода: {}", e)
                }
                OrchestratorError::PlaybackDeviceError(e) => {
                    error!("Ошибка при получении устройства вывода: {}", e)
                }
                OrchestratorError::PlaybackConfigError(e) => {
                    error!("Ошибка при получении конфигурации устройства вывода: {}", e)
                }
                OrchestratorError::PlaybackStreamError(e) => {
                    error!("Ошибка при получении потока устройства ввода: {}", e)
                }
                OrchestratorError::ModelLoadError(e) => error!(e),
                OrchestratorError::RecognizerError(e) => {
                    error!("Ошибка при создании распознавателя: {}", e)
                } // _ => error!("Неизвестная ошибка..."),
            }
            return ExitCode::FAILURE;
        }
    };

    match orchestrator.start_streams() {
        Ok(()) => (),
        Err(err) => {
            error!("Ошибка запуска потоков: {}", err.to_string());
            return ExitCode::FAILURE;
        }
    };

    let running = orchestrator.running_handle();

    setup_shutdown_signal(running.clone());

    info!("audio-censor успешно запущен, для остановки нажмите Ctrl+C");
    orchestrator.run(); // блокирует текущий поток, пока running == true

    info!("Получен сигнал остановки, завершаю обработку хвостов...");
    orchestrator.shutdown();

    info!("Завершено");

    ExitCode::SUCCESS
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

fn setup_shutdown_signal(running: Arc<AtomicBool>) {
    ctrlc::set_handler(move || {
        info!("Получен сигнал завершения (Ctrl+C / SIGTERM)");
        running.store(false, Ordering::Relaxed);
    })
    .expect("Не удалось установить обработчик сигнала завершения");
}
