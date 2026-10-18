use audio_censor::config::Settings;
use audio_censor::control::{self, ControlCommand, ControlHandle};
use audio_censor::logging::{init_logger, logs_to_file};
use audio_censor::pipeline::Orchestrator;
use audio_censor::{capture, playback};

use clap::{Parser, ValueEnum};
use cpal::SizedSample;
use dasp::{Sample, sample::Duplex};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
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
    /// Команда управления уже запущенным демоном (если не указана — запускается сам демон)
    #[command(subcommand)]
    command: Option<ControlCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DeviceFilter {
    Input,
    Output,
    All,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // 1. Просмотр устройств
    if let Some(filter) = cli.show_device {
        show_device(filter);
        return ExitCode::SUCCESS;
    }

    // Логгер ещё не инициализирован — ошибку видно только через stderr
    let settings = match Settings::new(cli.config_path) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("Ошибка получения конфигурации: {}", err);
            return ExitCode::FAILURE;
        }
    };

    if let Err(err) = init_logger(&settings.logging) {
        eprintln!("Ошибка инициализации логгера: {}", err);
        return ExitCode::FAILURE;
    }
    // 2. Команда управления запущенным демоном — короткоживущий клиент, не сам демон
    if let Some(cmd) = cli.command {
        return run_control_client(&settings.control.socket_path, cmd);
    }

    if let Err(err) = settings.check() {
        return fatal(format!("Ошибка конфигурации: {}", err));
    }

    // 3. Если аргументы вообще не переданы — запускаем демона
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
        other => fatal(format!("Неподдерживаемый формат сэмпла: {other}")),
    }
}

fn start<T>(settings: Settings) -> ExitCode
where
    T: Copy + Sample + Duplex<f32> + SizedSample + Send + 'static,
{
    info!("Запуск audio-censor");
    let socket_path = settings.control.socket_path.clone();
    let enabled = settings.control.enabled;
    let metrics_file = settings.metrics.metrics_file.clone();
    let mut orchestrator = match Orchestrator::<T>::new(settings) {
        Ok(o) => o,
        Err(err) => return fatal(err),
    };

    if let Err(err) = orchestrator.start_streams() {
        return fatal(format!("Ошибка запуска потоков: {}", err));
    }

    let running = orchestrator.running_handle();

    // Без обработчика демон нельзя корректно остановить: хвосты и метрики не сохранятся
    if let Err(err) = setup_shutdown_signal(running.clone()) {
        return fatal(format!(
            "Не удалось установить обработчик сигнала завершения: {}",
            err
        ));
    }

    let control_handle = ControlHandle::new(
        orchestrator.running_handle(),
        orchestrator.metrics_handle(),
        orchestrator.dict_reload_handle(),
        metrics_file,
    );

    if enabled {
        thread::spawn(move || {
            if let Err(e) = control::ipc::run_control_server(&socket_path, control_handle) {
                error!("Control-сервер завершился с ошибкой: {}", e);
            }
        });
    }
    info!("audio-censor успешно запущен, для остановки нажмите Ctrl+C");
    orchestrator.run();

    info!("Получен сигнал остановки, завершаю обработку хвостов...");
    orchestrator.shutdown();

    info!("Завершено");

    ExitCode::SUCCESS
}

/// Фатальная ошибка запуска: в лог, а если лог пишется в файл — ещё и в stderr,
/// иначе пользователь в терминале увидел бы только код возврата
fn fatal(message: impl std::fmt::Display) -> ExitCode {
    error!("{}", message);
    if logs_to_file() {
        eprintln!("{}", message);
    }
    ExitCode::FAILURE
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

fn setup_shutdown_signal(running: Arc<AtomicBool>) -> Result<(), ctrlc::Error> {
    ctrlc::set_handler(move || {
        info!("Получен сигнал завершения (Ctrl+C / SIGTERM)");
        running.store(false, Ordering::Relaxed);
    })
}

fn run_control_client(socket_path: &str, cmd: ControlCommand) -> ExitCode {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = match UnixStream::connect(socket_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Не удалось подключиться к демону по адресу '{}': {} (демон запущен?)",
                socket_path, e
            );
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = writeln!(stream, "{}", cmd.as_wire()) {
        eprintln!("Ошибка отправки команды: {}", e);
        return ExitCode::FAILURE;
    }

    let mut reader = BufReader::new(stream);
    let mut response = String::new();

    match reader.read_line(&mut response) {
        Ok(0) => {
            eprintln!("Демон закрыл соединение, не прислав ответ");
            ExitCode::FAILURE
        }
        Ok(_) => {
            println!("{}", response.trim());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Ошибка чтения ответа: {}", e);
            ExitCode::FAILURE
        }
    }
}
