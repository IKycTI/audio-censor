#[derive(Debug)]
pub struct TimedWord {
    pub word: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

impl TimedWord {
    pub fn new(word: &str, start_ms: u64, end_ms: u64) -> Self {
        Self {
            word: word.to_string(),
            start_ms,
            end_ms,
        }
    }
}
