use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use cpal::{traits::StreamTrait, Sample, SizedSample, Stream};
use dasp::sample::Duplex;
use ringbuf::{
    traits::{Consumer, Producer},
    HeapCons, HeapProd,
};
use tracing::{debug, error, info};
use vosk::Model;

use crate::{
    buffer::{self, DelayBuffer},
    capture,
    config::{BanSet, Settings},
    decision::DecisionEngine,
    framing::{convert_to_i16_mono, Chunker},
    masking::{Beep, Masker, Masking, MaskingStrategy, MuteAttenuate, Silence},
    playback,
    recognition::{SpeechRecognizer, TimedWord},
    MS_PER_SEC,
};

const EXTEND_RAW_BUF: usize = 100;

pub struct Orchestrator<T>
where
    T: Default + Copy + Sample + Duplex<f32>,
{
    consumer: HeapCons<T>,
    producer: HeapProd<T>,
    chunker: Chunker,
    _model: Model,
    recognizer: SpeechRecognizer,
    engine: DecisionEngine,
    delay_buffer: DelayBuffer<T>,
    masker: Masker<T>,
    capture_stream: Stream,
    playback_stream: Stream,
    channels: u16,
    running: Arc<AtomicBool>,
    dropped_output_samples: Arc<AtomicU64>, // метрика — переполнение output-буфера (DropNewest)
    raw_read_buf: Vec<T>,
    output_drain_wait: Duration,
}

impl<T> Orchestrator<T>
where
    T: Default + Copy + Sample + Duplex<f32> + SizedSample + Send + 'static,
{
    pub fn new(settings: Settings) -> Result<Self, OrchestratorError> {
        info!(
            "Загрузка словаря запрещенных слов из {}...",
            settings.decision.words_dict_path
        );
        let word_dict = match BanSet::new(&settings.decision.words_dict_path) {
            Ok(dict) => {
                info!("Словарь загружен");
                dict
            }
            Err(e) => {
                return Err(OrchestratorError::WordDictError(e));
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

        let chunker = Chunker::new(
            settings.framing.chunk_size_ms,
            settings.framing.overlap_ms,
            settings.audio.sample_rate,
        );

        let recognizer = match SpeechRecognizer::new(&model, settings.audio.sample_rate, 0) {
            Ok(r) => r,
            Err(err) => {
                return Err(OrchestratorError::RecognizerError(err));
            }
        };

        let engine = DecisionEngine::new(word_dict.words, settings.decision.max_distance);

        let delay_buffer = DelayBuffer::new(settings.buffer.delay_ms, settings.audio.sample_rate);

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
        );

        let raw_read_buf_capacity =
            (settings.audio.sample_rate as f32 / MS_PER_SEC) as usize * EXTEND_RAW_BUF;
        info!("Размер буфера сырых сэмплов: {}", raw_read_buf_capacity);

        let output_drain_wait =
            Duration::from_millis(settings.buffer.ring_capacity_ms as u64 + 50u64);

        let (input_producer, input_consumer) =
            buffer::create_scsp::<T>(settings.buffer.ring_capacity_ms);

        let (output_producer, output_consumer) =
            buffer::create_scsp::<T>(settings.buffer.ring_capacity_ms);

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

        let drop_input: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));

        let input_stream = match capture::build_capture_stream(
            input_device,
            input_config,
            input_producer,
            drop_input,
        ) {
            Ok(stream) => stream,
            Err(e) => {
                return Err(OrchestratorError::CaptureStreamError(e.to_string()));
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

        let drop_output: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));

        let output_stream = match playback::build_playback_stream(
            output_device,
            output_config,
            output_consumer,
            drop_output,
        ) {
            Ok(stream) => stream,
            Err(e) => {
                return Err(OrchestratorError::PlaybackStreamError(e.to_string()));
            }
        };

        Ok(Orchestrator {
            consumer: input_consumer,
            producer: output_producer,
            chunker,
            _model: model,
            recognizer,
            engine,
            delay_buffer,
            masker,
            capture_stream: input_stream,
            playback_stream: output_stream,
            channels: settings.audio.channels,
            running: Arc::new(AtomicBool::new(true)),
            dropped_output_samples: Arc::new(AtomicU64::new(0)),
            raw_read_buf: vec![T::default(); raw_read_buf_capacity],
            output_drain_wait,
        })
    }
    pub fn start_streams(&self) -> Result<(), cpal::PlayStreamError> {
        info!("Запуск потоков вводы и вывода");
        self.capture_stream.play()?;
        info!("Поток ввода запушен");
        self.playback_stream.play()?;
        info!("Поток вывода запушен");
        Ok(())
    }
    pub fn run(&mut self) {
        while self.running.load(Ordering::Relaxed) {
            // 1. Читаем всё, что успело накопиться в input ring buffer
            let n = self.consumer.pop_slice(&mut self.raw_read_buf);

            if n == 0 {
                // info!("В буфере ввода пусто");
                thread::sleep(Duration::from_millis(2));
                continue;
            }
            info!("Прочитано {} сэмплов из input buffer", n);

            let raw_samples = &self.raw_read_buf[..n];
            let len_rs = raw_samples.len();
            // 2. Сырые сэмплы (в исходном типе T) — сразу в delay_buffer,
            //    независимо от того, что с ними будет решено дальше
            info!("В delay_buffer отправлено {} сырых семплов", len_rs);
            self.delay_buffer.push(raw_samples);

            // 3. Параллельно — конвертация в i16-моно для распознавания
            info!("На конвертацию отправлено {} сырых семплов", len_rs);
            let converted = convert_to_i16_mono(raw_samples, self.channels);
            let len_c = converted.len();
            info!("Сконвертировано {} семплов", len_c);

            // 4. Нарезка на чанки нужного размера
            let chunks = self.chunker.push_samples(&converted);
            let len_chunks = chunks.len();
            info!("Создано чанков: {}", len_chunks);

            // 5. Каждый готовый чанк — в распознаватель, затем в decision engine
            for chunk in chunks {
                let words = self.recognizer.push_chunk(&chunk);
                let len_w = words.len();
                info!("Получено {} слов", len_w);
                self.handle_words(&words);
            }

            // 6. Забираем из delay_buffer то, что уже "отлежало" delay_ms
            let ready = self.delay_buffer.pop_ready();
            let len_r = ready.len();
            info!("Cемлов готово: {}", len_r);

            if ready.is_empty() {
                continue;
            }

            // 7. Применяем маску (или passthrough) к готовым сэмплам
            info!("Применение маски...");
            let processed = self.masker.process(&ready);

            // 8. Отдаём обработанные сэмплы в output ring buffer
            let pushed = self.producer.push_slice(&processed);
            info!("Отдано на выход: {}", pushed);
            if pushed < processed.len() {
                self.dropped_output_samples
                    .fetch_add((processed.len() - pushed) as u64, Ordering::Relaxed);
            }
        }
    }
    pub fn shutdown(mut self) {
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

            let converted = convert_to_i16_mono(raw_samples, self.channels);
            let chunks = self.chunker.push_samples(&converted);

            for chunk in chunks {
                let words = self.recognizer.push_chunk(&chunk);
                self.handle_words(&words);
            }
        }

        // 3. Дожать хвост чанкера — неполный кусок, оставшийся во внутреннем накопителе
        if let Some(tail_chunk) = self.chunker.finish() {
            let words = self.recognizer.push_chunk(&tail_chunk);
            self.handle_words(&words);
        }

        // 4. Дожать хвост распознавателя — незавершённая фраза (finish потребляет recognizer)
        let final_words = self.recognizer.finish();
        self.handle_words(&final_words);

        // 5. Дожать хвост delay_buffer — все сэмплы, включая те, что не отлежали полный delay_ms
        //    (finish потребляет delay_buffer)
        let tail_samples = self.delay_buffer.finish();
        if !tail_samples.is_empty() {
            let processed = self.masker.process(&tail_samples);
            let pushed = self.producer.push_slice(&processed);
            if pushed < processed.len() {
                self.dropped_output_samples
                    .fetch_add((processed.len() - pushed) as u64, Ordering::Relaxed);
            }
        }

        // 6. Дать playback-потоку время доиграть то, что осталось в output ring buffer,
        //    прежде чем останавливать сам playback-поток
        thread::sleep(self.output_drain_wait);

        if let Err(e) = self.playback_stream.pause() {
            error!("Не удалось остановить выходной поток: {}", e);
        }

        info!(
            dropped_output_samples = self.dropped_output_samples.load(Ordering::Relaxed),
            "Оркестратор остановлен"
        );
    }

    pub fn running_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.running)
    }

    fn handle_words(&mut self, words: &[TimedWord]) {
        if words.is_empty() {
            return;
        }

        debug!("Распознанные слова: \n {:?}", words);

        if let Some(intervals) = self.engine.get_batch_ban_time(words) {
            for interval in intervals {
                self.masker.add_interval(&interval);
            }
        }
    }
}

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
