use tracing::{error, info};
use vosk::{DecodingState, Model, Recognizer, Word};

use crate::{framing::Chunk, recognition::TimedWord, MS_PER_SEC};

pub struct SpeechRecognizer {
    recognizer: Option<Recognizer>,
    _sample_rate: f32,
    stream_offset_ms: u64,
}

impl SpeechRecognizer {
    pub fn new(model: &Model, sample_rate: f32, stream_offset_ms: u64) -> Result<Self, String> {
        if let Some(mut recognizer) = Recognizer::new(model, sample_rate) {
            recognizer.set_words(true);
            Ok(SpeechRecognizer {
                recognizer: Some(recognizer),
                _sample_rate: sample_rate,
                stream_offset_ms,
            })
        } else {
            Err("Не удалось создать распознаватель".to_string())
        }
    }

    pub fn push_chunk(&mut self, chunk: &Chunk) -> Vec<TimedWord> {
        let mut recognizer = self.recognizer.take().expect("Recognizer уже был завершён");

        let accept_res = recognizer.accept_waveform(&chunk.data);

        match accept_res {
            Ok(res) => match res {
                DecodingState::Finalized => {
                    info!("Чанк добален, слова были распознаны");

                    let words = get_words(&mut recognizer);
                    self.convert_word_to_timed_word(words)
                }
                DecodingState::Running => {
                    info!("Чанк добавлен, слова не распознаны");
                    Vec::new()
                }
                DecodingState::Failed => {
                    error!("Не удалось добавить чанк");
                    Vec::new()
                }
            },
            Err(err) => {
                error!("Ошибка придобавлении чанка: {}", err.to_string());
                return Vec::new();
            }
        }
    }

    pub fn finish(mut self) -> Vec<TimedWord> {
        let mut recognizer = self.recognizer.take().expect("Recognizer уже был завершён");
        let words = get_words(&mut recognizer);

        self.convert_word_to_timed_word(words)
    }

    fn convert_word_to_timed_word(&self, words: Vec<Word<'_>>) -> Vec<TimedWord> {
        words
            .iter()
            .map(|w| {
                let start = self.stream_offset_ms + (w.start * MS_PER_SEC).round() as u64;
                let end = self.stream_offset_ms + (w.end * MS_PER_SEC).round() as u64;
                TimedWord::new(w.word, start, end)
            })
            .collect()
    }
}

fn get_words(recognizer: &mut Recognizer) -> Vec<Word<'_>> {
    let result = recognizer
        .final_result()
        .single()
        .expect("При получении результатов Vosk произошла ошибка");
    let words = result.result;
    words
}
