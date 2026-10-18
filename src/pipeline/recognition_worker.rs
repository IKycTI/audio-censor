use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use tracing::{debug, error, info};
use vosk::Model;

use crate::{
    decision::{DecisionEngine, IntervalBanTime},
    framing::{Chunk, Chunker},
    metrics::Metrics,
    pipeline::orchestrator::{OrchestratorError, load_banned_words},
    recognition::{SpeechRecognizer, TimedWord},
};

// Как часто поток проверяет флаг перезагрузки словаря, если звук не приходит (пауза)
const DICT_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Поток распознавания: владеет Chunker, SpeechRecognizer и DecisionEngine.
/// Принимает моно-сэмплы i16 и возвращает интервалы запрещённых слов (в мс),
/// чтобы декодирование Vosk не блокировало цикл, выдающий звук
pub struct RecognitionWorker {
    samples_tx: Option<Sender<Vec<i16>>>,
    intervals_rx: Receiver<Vec<IntervalBanTime>>,
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
    intervals_tx: Sender<Vec<IntervalBanTime>>,
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
        let (intervals_tx, intervals_rx) = mpsc::channel::<Vec<IntervalBanTime>>();

        let state = WorkerState {
            _model: model,
            chunker,
            recognizer,
            engine,
            dict_path,
            dict_reload_req,
            metrics,
            intervals_tx,
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
    pub fn try_recv_intervals(&self) -> Vec<IntervalBanTime> {
        self.intervals_rx.try_iter().flatten().collect()
    }

    /// Закрыть канал, дождаться, пока поток дораспознает хвост, и забрать последние интервалы
    pub fn finish(mut self) -> Vec<IntervalBanTime> {
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
            Ok(final_words) => self.handle_words(&final_words),
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
        match self.recognizer.push_chunk(chunk) {
            Ok(words) => self.handle_words(&words),
            Err(e) => {
                error!("Ошибка распознавания: {}", e);
                self.metrics
                    .recognizer_errors
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn handle_words(&mut self, words: &[TimedWord]) {
        if words.is_empty() {
            return;
        }

        info!("Получено {} слов", words.len());
        debug!("Распознанные слова: \n {:?}", words);
        self.metrics
            .words_recognized
            .fetch_add(words.len() as u64, Ordering::Relaxed);

        if let Some(intervals) = self.engine.get_batch_ban_time(words) {
            self.metrics
                .words_censored
                .fetch_add(intervals.len() as u64, Ordering::Relaxed);
            // Оркестратор уже завершился — интервалы некому применять
            let _ = self.intervals_tx.send(intervals);
        }
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
