use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use cpal::{Sample, SizedSample, Stream, traits::StreamTrait};
use dasp::sample::Duplex;
use ringbuf::{
    HeapCons, HeapProd,
    traits::{Consumer, Observer, Producer},
};
use tracing::{error, info, trace, warn};
use vosk::Model;

use crate::{
    buffer::{self, DelayBuffer},
    capture,
    config::{BanSet, Settings},
    decision::{DecisionEngine, DetectedInterval},
    framing::{Chunker, convert_to_i16_mono},
    masking::{Beep, Masker, Masking, MaskingStrategy, MuteAttenuate, Silence},
    metrics::{Metrics, MetricsSnapshot},
    ms_to_stream_pos,
    pipeline::{DriftCompensator, DriftCorrection, OUTPUT_TARGET_FILL_MS, RecognitionWorker},
    playback,
    recognition::SpeechRecognizer,
    stream_pos_to_ms, whole_frames,
};

// Сколько звука забирать из входного буфера за одну итерацию цикла
const RAW_READ_BUF_MS: u64 = 100;
// Как часто при завершении проверять, освободилось ли место в выходном буфере
const OUTPUT_POLL_INTERVAL: Duration = Duration::from_millis(5);
// Сколько ждать после опустошения выходного буфера, пока доиграет буфер устройства
const DEVICE_DRAIN_WAIT: Duration = Duration::from_millis(200);

pub struct Orchestrator<T>
where
    T: Copy + Sample + Duplex<f32>,
{
    consumer: HeapCons<T>,
    producer: HeapProd<T>,
    recognition: RecognitionWorker,
    delay_buffer: DelayBuffer<T>,
    masker: Masker<T>,
    drift: DriftCompensator,
    capture_stream: Stream,
    playback_stream: Stream,
    count_underruns: Arc<AtomicBool>,
    sample_rate: u32,
    channels: u16,
    running: Arc<AtomicBool>,
    dict_reload_req: Arc<AtomicBool>,
    metrics_enabled: bool,
    metrics: Arc<Metrics>,
    raw_read_buf: Vec<T>,
    last_metrics_export: Instant,
    // None — периодическое логирование метрик выключено (export_interval_ms = 0)
    metrics_export_interval: Option<Duration>,
    output_drain_wait: Duration,
}

impl<T> Orchestrator<T>
where
    T: Copy + Sample + Duplex<f32> + SizedSample + Send + 'static,
{
    pub fn new(settings: Settings) -> Result<Self, OrchestratorError> {
        info!(
            "Загрузка словаря запрещенных слов из {}...",
            settings.decision.words_dict_path
        );
        let word_dict = load_banned_words(&settings.decision.words_dict_path)?;
        // Сначала всё, что проверяется быстро и чаще всего ошибочно (устройства и их
        // параметры), и только потом долгая загрузка модели: иначе об опечатке в имени
        // устройства пользователь узнал бы лишь после загрузки Vosk
        let input_device = match capture::get_device(&settings.audio.input_device) {
            Ok(device) => device,
            Err(e) => {
                return Err(OrchestratorError::CaptureDeviceError(e));
            }
        };

        let input_config = match capture::get_config(&input_device, &settings.audio) {
            Ok(config) => config,
            Err(e) => {
                return Err(OrchestratorError::CaptureConfigError(e));
            }
        };

        let output_device = match playback::get_device(&settings.audio.output_device) {
            Ok(device) => device,
            Err(e) => {
                return Err(OrchestratorError::PlaybackDeviceError(e));
            }
        };

        let output_config = match playback::get_config(&output_device, &settings.audio) {
            Ok(config) => config,
            Err(e) => {
                return Err(OrchestratorError::PlaybackConfigError(e));
            }
        };

        info!("Загрузка модели из {}...", settings.recognition.model_path);
        let model = match Model::new(&settings.recognition.model_path) {
            Some(m) => {
                info!("Модель загружена");
                m
            }
            None => {
                let err = format!(
                    "Не удалось загрузить модель по пути '{}' — проверьте путь в конфиге",
                    settings.recognition.model_path
                );
                return Err(OrchestratorError::ModelLoadError(err));
            }
        };

        let metrics = Arc::new(Metrics::default());

        let chunker = Chunker::new(settings.framing.chunk_size_ms, settings.audio.sample_rate);

        let recognizer = match SpeechRecognizer::new(
            &model,
            settings.audio.sample_rate,
            0,
            settings.recognition.partial_results,
        ) {
            Ok(r) => r,
            Err(err) => {
                return Err(OrchestratorError::RecognizerError(err));
            }
        };

        let engine = DecisionEngine::new(
            word_dict.words,
            settings.decision.max_distance,
            settings.recognition.min_confidence,
        );

        // Распознавание — в отдельном потоке, чтобы декодирование Vosk не задерживало выдачу звука
        let dict_reload_req = Arc::new(AtomicBool::new(false));
        let recognition = RecognitionWorker::spawn(
            model,
            chunker,
            recognizer,
            engine,
            settings.decision.words_dict_path.clone(),
            Arc::clone(&dict_reload_req),
            Arc::clone(&metrics),
        )?;

        let delay_buffer = DelayBuffer::new(
            settings.buffer.delay_ms,
            settings.audio.sample_rate,
            settings.audio.channels,
        );

        let strategy: Box<dyn Masking<T>> = match settings.masking.strategy {
            MaskingStrategy::Beep => Box::new(Beep::new(
                settings.masking.beep_frequency_hz,
                settings.masking.amplitude,
            )),
            MaskingStrategy::Silence => Box::new(Silence::new()),
            MaskingStrategy::MuteAttenuate => {
                Box::new(MuteAttenuate::new(settings.masking.attenuation_db))
            }
        };

        let masker = Masker::new(
            strategy,
            settings.masking.padding_ms,
            settings.audio.sample_rate,
            settings.audio.channels,
        );

        let raw_read_buf_capacity = ms_to_stream_pos(
            RAW_READ_BUF_MS,
            settings.audio.sample_rate,
            settings.audio.channels,
        ) as usize;
        info!("Размер буфера сырых сэмплов: {}", raw_read_buf_capacity);

        let output_drain_wait =
            Duration::from_millis(settings.buffer.ring_capacity_ms as u64 + 50u64);

        let ring_capacity = ms_to_stream_pos(
            settings.buffer.ring_capacity_ms as u64,
            settings.audio.sample_rate,
            settings.audio.channels,
        ) as usize;

        let (input_producer, input_consumer) = buffer::create_scsp::<T>(ring_capacity);

        let (output_producer, output_consumer) = buffer::create_scsp::<T>(ring_capacity);

        // cpal-потоки создаются последними: на некоторых бэкендах поток начинает работать
        // уже при создании, и захват не должен копить звук, пока грузится модель
        let input_stream = match capture::build_capture_stream(
            input_device,
            input_config,
            input_producer,
            Arc::clone(&metrics),
        ) {
            Ok(stream) => stream,
            Err(e) => {
                return Err(OrchestratorError::CaptureStreamError(e.to_string()));
            }
        };

        let count_underruns = Arc::new(AtomicBool::new(true));
        let output_stream = match playback::build_playback_stream(
            output_device,
            output_config,
            output_consumer,
            Arc::clone(&metrics),
            Arc::clone(&count_underruns),
        ) {
            Ok(stream) => stream,
            Err(e) => {
                return Err(OrchestratorError::PlaybackStreamError(e.to_string()));
            }
        };

        Ok(Orchestrator {
            consumer: input_consumer,
            producer: output_producer,
            recognition,
            delay_buffer,
            masker,
            drift: DriftCompensator::new(
                OUTPUT_TARGET_FILL_MS,
                settings.audio.sample_rate,
                settings.audio.channels,
            ),
            capture_stream: input_stream,
            playback_stream: output_stream,
            count_underruns,
            sample_rate: settings.audio.sample_rate,
            channels: settings.audio.channels,
            running: Arc::new(AtomicBool::new(true)),
            dict_reload_req,
            metrics_enabled: settings.metrics.enabled,
            metrics: Arc::clone(&metrics),
            raw_read_buf: vec![T::EQUILIBRIUM; raw_read_buf_capacity],
            last_metrics_export: Instant::now(),
            metrics_export_interval: (settings.metrics.export_interval_ms > 0)
                .then(|| Duration::from_millis(settings.metrics.export_interval_ms)),
            output_drain_wait,
        })
    }

    pub fn start_streams(&self) -> Result<(), cpal::PlayStreamError> {
        info!("Запуск потоков ввода и вывода");
        self.capture_stream.play()?;
        info!("Поток ввода запущен");
        self.playback_stream.play()?;
        info!("Поток вывода запущен");
        Ok(())
    }

    pub fn run(&mut self) {
        while self.running.load(Ordering::Relaxed) {
            if self.metrics_enabled
                && let Some(interval) = self.metrics_export_interval
                && self.last_metrics_export.elapsed() >= interval
            {
                log_metrics(&self.metrics.snapshot(), "метрики");
                self.last_metrics_export = Instant::now();
            }
            // 1. Читаем всё, что успело накопиться в input ring buffer
            let n = self.consumer.pop_slice(&mut self.raw_read_buf);

            if n == 0 {
                thread::sleep(Duration::from_millis(2));
                continue;
            }
            trace!("Прочитано {} сэмплов из input buffer", n);

            let raw_samples = &self.raw_read_buf[..n];
            let len_rs = raw_samples.len();
            // 2. Сырые сэмплы (в исходном типе T) — сразу в delay_buffer,
            //    независимо от того, что с ними будет решено дальше
            trace!("В delay_buffer отправлено {} сырых сэмплов", len_rs);
            self.delay_buffer.push(raw_samples);
            self.metrics.stream_duration_ms.store(
                stream_pos_to_ms(
                    self.delay_buffer.input_position(),
                    self.sample_rate,
                    self.channels,
                ),
                Ordering::Relaxed,
            );

            // 3. Параллельно — конвертация в i16-моно и отправка в поток распознавания
            //    (нарезка на чанки, Vosk и decision engine работают там)
            trace!("На конвертацию отправлено {} сырых сэмплов", len_rs);
            let converted = convert_to_i16_mono(raw_samples, self.channels);
            trace!("Сконвертировано {} сэмплов", converted.len());
            self.recognition.send(converted);

            // 4. Регистрируем интервалы, которые поток распознавания успел найти
            self.add_ready_intervals();

            // 5. Забираем из delay_buffer то, что уже "отлежало" delay_ms
            let ready = self.delay_buffer.pop_ready();
            let len_r = ready.len();
            trace!("Сэмплов готово: {}", len_r);

            if ready.is_empty() {
                continue;
            }

            // 6. Применяем маску (или passthrough) к готовым сэмплам
            trace!("Применение маски...");
            let mut processed = self.masker.process(&ready);

            // 7. Компенсация дрейфа часов: держим заполненность выходного буфера около цели
            let fill = self.producer.occupied_len();
            match self.drift.adjust(&mut processed, fill, T::EQUILIBRIUM) {
                DriftCorrection::None => {}
                DriftCorrection::Prefilled(frames) => {
                    // Первое заполнение — старт вывода; повторное — буфер опустел посреди
                    // работы, звук прерывался: это не дрейф, а срыв
                    let before = self.metrics.output_prefills.fetch_add(1, Ordering::Relaxed);
                    self.metrics
                        .output_prefill_frames
                        .fetch_add(frames as u64, Ordering::Relaxed);
                    if before == 0 {
                        info!("Старт вывода: добавлено {} кадров тишины", frames);
                    } else {
                        warn!(
                            "Выходной буфер опустел посреди работы: добавлено {} кадров тишины",
                            frames
                        );
                    }
                }
                DriftCorrection::DroppedFrame => {
                    self.metrics
                        .drift_dropped_frames
                        .fetch_add(1, Ordering::Relaxed);
                }
                DriftCorrection::DuplicatedFrame => {
                    self.metrics
                        .drift_inserted_frames
                        .fetch_add(1, Ordering::Relaxed);
                }
            }

            // 8. Отдаём обработанные сэмплы в output ring buffer
            push_output(&mut self.producer, &self.metrics, self.channels, &processed);
        }
    }

    pub fn shutdown(mut self) {
        // Дальше выходной буфер неизбежно пустеет: пока поток распознавания дожимает
        // последние слова, хвост задержки выводить нельзя. Это обрыв при остановке,
        // а не потери в работе — в underrun_samples его не считаем
        self.count_underruns.store(false, Ordering::Relaxed);

        // 1. Остановить приём новых данных — дальше в input ring buffer ничего не добавится
        if let Err(e) = self.capture_stream.pause() {
            error!("Не удалось остановить входной поток: {}", e);
        }

        // 2. Финальный проход — дочитать всё, что уже физически лежит в input buffer,
        //    ровно той же логикой, что и обычная итерация run(), но один раз, без ожидания новых данных
        loop {
            let n = self.consumer.pop_slice(&mut self.raw_read_buf);
            if n == 0 {
                break;
            }

            let raw_samples = &self.raw_read_buf[..n];
            self.delay_buffer.push(raw_samples);
            self.recognition
                .send(convert_to_i16_mono(raw_samples, self.channels));
        }

        // 3–4. Закрыть канал распознавания: поток дожимает хвост чанкера и незавершённую
        //      фразу, после join забираем последние интервалы (finish потребляет поток)
        let last_intervals = self.recognition.finish();
        register_intervals(&mut self.masker, &self.metrics, last_intervals, None);

        // 5. Дожать хвост delay_buffer — все сэмплы, включая те, что не отлежали полный delay_ms
        //    (finish потребляет delay_buffer)
        //    Хвост — до delay_ms звука, он может не влезть в выходной буфер целиком,
        //    поэтому пишем порциями по мере того, как playback освобождает место
        let tail_samples = self.delay_buffer.finish();
        if !tail_samples.is_empty() {
            let processed = self.masker.process(&tail_samples);
            let tail_duration = Duration::from_millis(
                processed.len() as u64 * 1000 / (self.sample_rate as u64 * self.channels as u64),
            );
            push_output_blocking(
                &mut self.producer,
                &self.metrics,
                self.channels,
                &processed,
                tail_duration + self.output_drain_wait,
            );
        }

        // 6. Дать playback-потоку доиграть то, что осталось в output ring buffer,
        //    прежде чем останавливать сам playback-поток
        let drain_start = Instant::now();
        while self.producer.occupied_len() > 0 && drain_start.elapsed() < self.output_drain_wait {
            thread::sleep(OUTPUT_POLL_INTERVAL);
        }
        // Последняя порция ещё звучит в буфере самого устройства
        thread::sleep(DEVICE_DRAIN_WAIT);

        if let Err(e) = self.playback_stream.pause() {
            error!("Не удалось остановить выходной поток: {}", e);
        }

        log_metrics(
            &self.metrics.snapshot(),
            "Оркестратор остановлен, итоговые метрики",
        );
    }

    pub fn running_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.running)
    }

    pub fn dict_reload_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.dict_reload_req)
    }

    pub fn metrics_handle(&self) -> Arc<Metrics> {
        Arc::clone(&self.metrics)
    }
    fn add_ready_intervals(&mut self) {
        let intervals = self.recognition.try_recv_intervals();
        if intervals.is_empty() {
            return;
        }
        let input_ms = stream_pos_to_ms(
            self.delay_buffer.input_position(),
            self.sample_rate,
            self.channels,
        );
        register_intervals(&mut self.masker, &self.metrics, intervals, Some(input_ms));
    }
}

pub(crate) fn load_banned_words(dict_path: &str) -> Result<BanSet, OrchestratorError> {
    let dict = BanSet::new(dict_path).map_err(OrchestratorError::WordDictError)?;
    info!("Словарь загружен");
    Ok(dict)
}

#[derive(Debug)]
pub enum OrchestratorError {
    WordDictError(String),
    CaptureDeviceError(String),
    CaptureConfigError(String),
    CaptureStreamError(String),
    PlaybackDeviceError(String),
    PlaybackConfigError(String),
    PlaybackStreamError(String),
    ModelLoadError(String),
    RecognizerError(String),
}

impl fmt::Display for OrchestratorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrchestratorError::WordDictError(e) => write!(f, "Ошибка при загрузке словаря: {}", e),
            OrchestratorError::CaptureDeviceError(e) => {
                write!(f, "Ошибка при получении устройства ввода: {}", e)
            }
            OrchestratorError::CaptureConfigError(e) => {
                write!(
                    f,
                    "Ошибка при получении конфигурации устройства ввода: {}",
                    e
                )
            }
            OrchestratorError::CaptureStreamError(e) => {
                write!(f, "Ошибка при создании потока устройства ввода: {}", e)
            }
            OrchestratorError::PlaybackDeviceError(e) => {
                write!(f, "Ошибка при получении устройства вывода: {}", e)
            }
            OrchestratorError::PlaybackConfigError(e) => {
                write!(
                    f,
                    "Ошибка при получении конфигурации устройства вывода: {}",
                    e
                )
            }
            OrchestratorError::PlaybackStreamError(e) => {
                write!(f, "Ошибка при создании потока устройства вывода: {}", e)
            }
            OrchestratorError::ModelLoadError(e) => write!(f, "{}", e),
            OrchestratorError::RecognizerError(e) => {
                write!(f, "Ошибка при создании распознавателя: {}", e)
            }
        }
    }
}

impl std::error::Error for OrchestratorError {}

fn log_metrics(snap: &MetricsSnapshot, message: &str) {
    info!(
        dropped_input = snap.dropped_input_samples,
        dropped_output = snap.dropped_output_samples,
        underruns = snap.underrun_samples,
        words_recognized = snap.words_recognized,
        words_censored = snap.words_censored,
        recognizer_errors = snap.recognizer_errors,
        late_intervals = snap.late_intervals,
        partial_intervals = snap.partial_intervals,
        unconfirmed_partial = snap.unconfirmed_partial_intervals,
        latency_avg_ms = snap.recognition_latency_avg_ms,
        latency_max_ms = snap.recognition_latency_max_ms,
        chunk_avg_us = snap.chunk_processing_avg_us,
        chunk_max_us = snap.chunk_processing_max_us,
        drift_dropped = snap.drift_dropped_frames,
        drift_inserted = snap.drift_inserted_frames,
        output_prefills = snap.output_prefills,
        output_prefill_frames = snap.output_prefill_frames,
        "{}",
        message
    );
}

/// Записать всё в выходной буфер, дожидаясь свободного места (только при завершении:
/// в рабочем цикле блокироваться нельзя). Что не влезло за `timeout`, считается потерянным
fn push_output_blocking<T>(
    producer: &mut HeapProd<T>,
    metrics: &Metrics,
    channels: u16,
    data: &[T],
    timeout: Duration,
) where
    T: Copy,
{
    let start = Instant::now();
    let mut rest = data;
    while !rest.is_empty() {
        let fits = whole_frames(producer.vacant_len().min(rest.len()), channels);
        let pushed = producer.push_slice(&rest[..fits]);
        rest = &rest[pushed..];

        if rest.is_empty() {
            break;
        }
        if start.elapsed() >= timeout {
            error!("Не удалось доиграть хвост: потеряно {} сэмплов", rest.len());
            metrics
                .dropped_output_samples
                .fetch_add(rest.len() as u64, Ordering::Relaxed);
            break;
        }
        thread::sleep(OUTPUT_POLL_INTERVAL);
    }
}

fn push_output<T>(producer: &mut HeapProd<T>, metrics: &Metrics, channels: u16, processed: &[T])
where
    T: Copy,
{
    // При переполнении пишем только целые кадры, иначе каналы на выходе поменяются местами
    let fits = whole_frames(producer.vacant_len().min(processed.len()), channels);
    let pushed = producer.push_slice(&processed[..fits]);
    trace!("Отдано на выход: {}", pushed);
    if pushed < processed.len() {
        metrics
            .dropped_output_samples
            .fetch_add((processed.len() - pushed) as u64, Ordering::Relaxed);
    }
}

/// Зарегистрировать интервалы в маске и учесть их своевременность.
/// `input_ms` — сколько мс звука уже принято на входе; `None` при завершении: там последние
/// слова выдаёт принудительный `recognizer.finish()`, и задержка распознавания не показательна
fn register_intervals<T>(
    masker: &mut Masker<T>,
    metrics: &Metrics,
    intervals: Vec<DetectedInterval>,
    input_ms: Option<u64>,
) where
    T: Copy + Sample + Duplex<f32>,
{
    for DetectedInterval { interval, is_new } in intervals {
        // Уточнение уже найденного слова (границы стали шире): только расширяем маску,
        // метрики и предупреждения уже учтены при первой находке
        if !is_new {
            masker.add_interval(&interval);
            continue;
        }

        let latency_ms = input_ms.map(|now| now.saturating_sub(interval.end_time));
        if let Some(latency) = latency_ms {
            metrics.record_recognition_latency(latency);
        }

        // Позиция выхода — до регистрации: после неё маска уже учитывает интервал
        let output_ms = masker.output_position_ms();
        let timing = masker.add_interval(&interval);
        if timing.is_late() {
            // Главный показатель того, хватает ли delay_ms на распознавание
            metrics.late_intervals.fetch_add(1, Ordering::Relaxed);
            // Чтобы слово заглушилось целиком, выход должен был отставать
            // ещё на столько: тогда начало слова было бы ещё в задержке
            let missing_delay_ms = output_ms.saturating_sub(interval.start_time);
            warn!(
                start_ms = interval.start_time,
                end_ms = interval.end_time,
                ?timing,
                latency_ms,
                missing_delay_ms,
                "Интервал опоздал: слово уже прозвучало"
            );
        }
    }
}
