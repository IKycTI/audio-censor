use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use tracing::{debug, error, info};
use vosk::Model;

use crate::{
    decision::{DecisionEngine, DetectedInterval, PhraseTracker, normalize_word},
    framing::{Chunk, Chunker},
    metrics::Metrics,
    pipeline::orchestrator::{OrchestratorError, load_banned_words},
    recognition::{Recognition, SpeechRecognizer, TimedWord},
};

// Как часто поток проверяет флаг перезагрузки словаря, если звук не приходит (пауза)
const DICT_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Поток распознавания: владеет Chunker, SpeechRecognizer и DecisionEngine.
/// Принимает моно-сэмплы i16 и возвращает интервалы запрещённых слов (в мс),
/// чтобы декодирование Vosk не блокировало цикл, выдающий звук
pub struct RecognitionWorker {
    samples_tx: Option<Sender<Vec<i16>>>,
    intervals_rx: Receiver<Vec<DetectedInterval>>,
    handle: Option<JoinHandle<()>>,
    disconnected: bool,
}

/// Состояние, которое живёт внутри потока распознавания
struct WorkerState {
    _model: Model,
    chunker: Chunker,
    recognizer: SpeechRecognizer,
    engine: DecisionEngine,
    dict_path: String,
    dict_reload_req: Arc<AtomicBool>,
    metrics: Arc<Metrics>,
    intervals_tx: Sender<Vec<DetectedInterval>>,
    phrase: PhraseTracker,
}

impl RecognitionWorker {
    pub fn spawn(
        model: Model,
        chunker: Chunker,
        recognizer: SpeechRecognizer,
        engine: DecisionEngine,
        dict_path: String,
        dict_reload_req: Arc<AtomicBool>,
        metrics: Arc<Metrics>,
    ) -> Result<Self, OrchestratorError> {
        let (samples_tx, samples_rx) = mpsc::channel::<Vec<i16>>();
        let (intervals_tx, intervals_rx) = mpsc::channel::<Vec<DetectedInterval>>();

        let state = WorkerState {
            _model: model,
            chunker,
            recognizer,
            engine,
            dict_path,
            dict_reload_req,
            metrics,
            intervals_tx,
            phrase: PhraseTracker::new(),
        };

        let handle = thread::Builder::new()
            .name("recognition".to_string())
            .spawn(move || state.run(samples_rx))
            .map_err(|e| OrchestratorError::RecognizerError(e.to_string()))?;

        info!("Поток распознавания запущен");

        Ok(RecognitionWorker {
            samples_tx: Some(samples_tx),
            intervals_rx,
            handle: Some(handle),
            disconnected: false,
        })
    }

    /// Отправить сконвертированные сэмплы на распознавание. Не блокирует
    pub fn send(&mut self, samples: Vec<i16>) {
        let Some(tx) = &self.samples_tx else {
            return;
        };
        if tx.send(samples).is_err() && !self.disconnected {
            // Поток распознавания упал: звук продолжит идти, но без цензуры
            error!("Поток распознавания завершился, распознавание остановлено");
            self.disconnected = true;
        }
    }

    /// Забрать все интервалы, готовые на данный момент. Не блокирует
    pub fn try_recv_intervals(&self) -> Vec<DetectedInterval> {
        self.intervals_rx.try_iter().flatten().collect()
    }

    /// Закрыть канал, дождаться, пока поток дораспознает хвост, и забрать последние интервалы
    pub fn finish(mut self) -> Vec<DetectedInterval> {
        // Закрытие канала — сигнал потоку доделать chunker.finish() и recognizer.finish()
        drop(self.samples_tx.take());

        if let Some(handle) = self.handle.take()
            && handle.join().is_err()
        {
            error!("Поток распознавания завершился с паникой");
        }

        self.try_recv_intervals()
    }
}

impl WorkerState {
    fn run(mut self, samples_rx: Receiver<Vec<i16>>) {
        loop {
            self.reload_dict_if_requested();

            match samples_rx.recv_timeout(DICT_POLL_INTERVAL) {
                Ok(samples) => {
                    for chunk in self.chunker.push_samples(&samples) {
                        self.push_chunk(&chunk);
                    }
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }

        // Канал закрыт: дожать хвост чанкера (неполный кусок) и незавершённую фразу
        if let Some(tail_chunk) = self.chunker.finish() {
            self.push_chunk(&tail_chunk);
        }
        match self.recognizer.finish() {
            Ok(final_words) => self.handle_final(&final_words),
            Err(e) => {
                error!("Не удалось получить последнюю фразу: {}", e);
                self.metrics
                    .recognizer_errors
                    .fetch_add(1, Ordering::Relaxed);
            }
        }

        info!("Поток распознавания остановлен");
    }

    fn push_chunk(&mut self, chunk: &Chunk) {
        // Декодирование и принятие решения вместе: это τ_proc из условия стабильности
        let started = Instant::now();
        match self.recognizer.push_chunk(chunk) {
            Ok(Recognition::Partial(words)) => self.handle_partial(&words),
            Ok(Recognition::Final(words)) => self.handle_final(&words),
            Err(e) => {
                error!("Ошибка распознавания: {}", e);
                self.metrics
                    .recognizer_errors
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        self.metrics
            .record_chunk_processing(started.elapsed().as_micros() as u64);
    }

    /// Фраза ещё идёт: запрещённое слово из гипотезы маскируем сразу, не дожидаясь паузы.
    /// Слова гипотезы не считаются в words_recognized — она повторяется на каждый чанк
    fn handle_partial(&mut self, words: &[TimedWord]) {
        let Some(intervals) = self.engine.get_batch_ban_time(words) else {
            return;
        };
        let detected = self.phrase.on_partial(intervals);
        let new = detected.iter().filter(|d| d.is_new).count() as u64;
        if new > 0 {
            debug!("Запрещённых слов в частичной гипотезе: {}", new);
            self.metrics
                .partial_intervals
                .fetch_add(new, Ordering::Relaxed);
        }
        self.send(detected);
    }

    /// Фраза завершена: окончательные слова, подтверждение находок из частичных гипотез
    fn handle_final(&mut self, words: &[TimedWord]) {
        if !words.is_empty() {
            // Распознанный текст — это речь пользователя, поэтому только на уровне debug
            debug!("Получено {} слов: {:?}", words.len(), words);
            self.metrics
                .words_recognized
                .fetch_add(words.len() as u64, Ordering::Relaxed);
        }
        // Статистика по словам — по окончательному результату: частичные находки
        // он подтверждает или отменяет, а слова из него и есть итог распознавания
        for word in words {
            if let Some(entry) = self.engine.matched_entry(word) {
                self.metrics
                    .record_censored_word(entry, &normalize_word(&word.word));
            }
        }

        let intervals = self.engine.get_batch_ban_time(words).unwrap_or_default();
        let outcome = self.phrase.on_final(intervals);
        if outcome.unconfirmed > 0 {
            debug!(
                "Не подтвердилось финальным результатом: {}",
                outcome.unconfirmed
            );
            self.metrics
                .unconfirmed_partial_intervals
                .fetch_add(outcome.unconfirmed as u64, Ordering::Relaxed);
        }
        self.send(outcome.detected);
    }

    fn send(&mut self, detected: Vec<DetectedInterval>) {
        if detected.is_empty() {
            return;
        }
        let new = detected.iter().filter(|d| d.is_new).count() as u64;
        self.metrics
            .words_censored
            .fetch_add(new, Ordering::Relaxed);
        // Оркестратор уже завершился — интервалы некому применять
        let _ = self.intervals_tx.send(detected);
    }

    fn reload_dict_if_requested(&mut self) {
        if !self.dict_reload_req.swap(false, Ordering::Relaxed) {
            return;
        }
        match load_banned_words(&self.dict_path) {
            Ok(new_dict) => {
                self.engine.replace_dictionary(new_dict.words);
                info!("Словарь перезагружен");
            }
            Err(e) => error!("Не удалось перезагрузить словарь: {}", e),
        }
    }
}
