use std::fmt;
use std::panic::{self, AssertUnwindSafe};

use tracing::{error, trace};
use vosk::{DecodingState, Model, Recognizer, Word};

use crate::{MS_PER_SEC, framing::Chunk, recognition::TimedWord};

pub struct SpeechRecognizer {
    recognizer: Recognizer,
    _sample_rate: u32,
    stream_offset_ms: u64,
    partial_words: bool,
}

/// Результат очередного чанка
#[derive(Debug)]
pub enum Recognition {
    /// Фраза продолжается: текущая гипотеза (пустая, если частичные гипотезы выключены).
    /// Может измениться с новыми чанками
    Partial(Vec<TimedWord>),
    /// Фраза завершена: окончательные слова
    Final(Vec<TimedWord>),
}

impl SpeechRecognizer {
    /// `partial_words` — отдавать слова с таймстемпами ещё до конца фразы
    /// (Vosk иначе выдаёт слова только после паузы, через секунды после их звучания)
    pub fn new(
        model: &Model,
        sample_rate: u32,
        stream_offset_ms: u64,
        partial_words: bool,
    ) -> Result<Self, String> {
        if let Some(mut recognizer) = Recognizer::new(model, sample_rate as f32) {
            recognizer.set_words(true);
            recognizer.set_partial_words(partial_words);
            Ok(SpeechRecognizer {
                recognizer,
                _sample_rate: sample_rate,
                stream_offset_ms,
                partial_words,
            })
        } else {
            Err("Не удалось создать распознаватель".to_string())
        }
    }

    pub fn push_chunk(&mut self, chunk: &Chunk) -> Result<Recognition, RecognizerError> {
        let accept_res = self.recognizer.accept_waveform(&chunk.data);

        match accept_res {
            Ok(res) => match res {
                DecodingState::Finalized => {
                    trace!("Чанк добавлен, фраза завершена");

                    // single() даёт None, только если включены альтернативы (set_max_alternatives)
                    let words = self
                        .recognizer
                        .result()
                        .single()
                        .ok_or(RecognizerError::UnexpectedResult)?;

                    Ok(Recognition::Final(convert_word_to_timed_word(
                        self.stream_offset_ms,
                        words.result,
                    )))
                }
                DecodingState::Running => {
                    trace!("Чанк добавлен, фраза продолжается");
                    self.partial()
                }
                // Ошибку логирует вызывающий (поток распознавания) — через Display
                DecodingState::Failed => Err(RecognizerError::DecodingFailed),
            },
            Err(err) => Err(RecognizerError::AcceptError(err.to_string())),
        }
    }

    fn partial(&mut self) -> Result<Recognition, RecognizerError> {
        if !self.partial_words {
            return Ok(Recognition::Partial(Vec::new()));
        }
        // Крейт vosk паникует, если JSON не совпал с его структурой (например, старая
        // libvosk без conf у частичных слов). Падение потока распознавания отключило бы
        // цензуру совсем — вместо этого выключаем только частичные гипотезы
        let offset = self.stream_offset_ms;
        let recognizer = &mut self.recognizer;
        let words = panic::catch_unwind(AssertUnwindSafe(|| {
            convert_word_to_timed_word(offset, recognizer.partial_result().partial_result)
        }));
        match words {
            Ok(words) => Ok(Recognition::Partial(words)),
            Err(_) => {
                error!("Частичные гипотезы Vosk не разбираются — отключены, работаем по фразам");
                self.partial_words = false;
                self.recognizer.set_partial_words(false);
                Err(RecognizerError::PartialResultUnavailable)
            }
        }
    }

    pub fn finish(&mut self) -> Result<Vec<TimedWord>, RecognizerError> {
        let words = self
            .recognizer
            .final_result()
            .single()
            .ok_or(RecognizerError::UnexpectedResult)?;

        Ok(convert_word_to_timed_word(
            self.stream_offset_ms,
            words.result,
        ))
    }
}

fn convert_word_to_timed_word(stream_offset_ms: u64, words: Vec<Word<'_>>) -> Vec<TimedWord> {
    words
        .iter()
        .map(|w| {
            let start = stream_offset_ms + (w.start * MS_PER_SEC).round() as u64;
            let end = stream_offset_ms + (w.end * MS_PER_SEC).round() as u64;
            TimedWord::new(w.word, start, end).with_conf(w.conf)
        })
        .collect()
}

#[derive(Debug)]
pub enum RecognizerError {
    DecodingFailed,
    AcceptError(String),
    UnexpectedResult,
    PartialResultUnavailable,
}

impl fmt::Display for RecognizerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecognizerError::DecodingFailed => write!(f, "Vosk не смог декодировать чанк"),
            RecognizerError::AcceptError(e) => write!(f, "ошибка при добавлении чанка: {}", e),
            RecognizerError::UnexpectedResult => {
                write!(
                    f,
                    "Vosk вернул результат с альтернативами вместо одиночного"
                )
            }
            RecognizerError::PartialResultUnavailable => {
                write!(f, "частичная гипотеза Vosk в неожиданном формате")
            }
        }
    }
}

impl std::error::Error for RecognizerError {}
