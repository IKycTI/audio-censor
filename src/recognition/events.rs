#[derive(Debug)]
pub struct TimedWord {
    pub word: String,
    pub start_ms: u64,
    pub end_ms: u64,
    // Уверенность Vosk в слове, 0.0–1.0
    pub conf: f32,
}

impl TimedWord {
    pub fn new(word: &str, start_ms: u64, end_ms: u64) -> Self {
        Self {
            word: word.to_string(),
            start_ms,
            end_ms,
            conf: 1.0,
        }
    }

    pub fn with_conf(mut self, conf: f32) -> Self {
        self.conf = conf;
        self
    }
}
