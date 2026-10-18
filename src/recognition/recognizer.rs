use std::fmt;

use tracing::info;
use vosk::{DecodingState, Model, Recognizer, Word};

use crate::{MS_PER_SEC, framing::Chunk, recognition::TimedWord};

pub struct SpeechRecognizer {
    recognizer: Recognizer,
    _sample_rate: u32,
    stream_offset_ms: u64,
}

impl SpeechRecognizer {
    pub fn new(model: &Model, sample_rate: u32, stream_offset_ms: u64) -> Result<Self, String> {
        if let Some(mut recognizer) = Recognizer::new(model, sample_rate as f32) {
            recognizer.set_words(true);
            Ok(SpeechRecognizer {
                recognizer: recognizer,
                _sample_rate: sample_rate,
                stream_offset_ms,
            })
        } else {
            Err("Не удалось создать распознаватель".to_string())
        }
    }

    pub fn push_chunk(&mut self, chunk: &Chunk) -> Result<Vec<TimedWord>, RecognizerError> {
        let accept_res = self.recognizer.accept_waveform(&chunk.data);

        match accept_res {
            Ok(res) => match res {
                DecodingState::Finalized => {
                    info!("Чанк добавлен, слова были распознаны");

                    // single() даёт None, только если включены альтернативы (set_max_alternatives)
                    let words = self
                        .recognizer
                        .result()
                        .single()
                        .ok_or(RecognizerError::UnexpectedResult)?;

                    Ok(convert_word_to_timed_word(
                        self.stream_offset_ms,
                        words.result,
                    ))
                }
                DecodingState::Running => {
                    info!("Чанк добавлен, слова не распознаны");
                    Ok(Vec::new())
                }
                // Ошибку логирует вызывающий (поток распознавания) — через Display
                DecodingState::Failed => Err(RecognizerError::DecodingFailed),
            },
            Err(err) => Err(RecognizerError::AcceptError(err.to_string())),
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
        }
    }
}

impl std::error::Error for RecognizerError {}
