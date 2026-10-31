use std::collections::HashSet;

use crate::decision::Rules;
use crate::recognition::TimedWord;

pub struct DecisionEngine {
    rules: Rules,
    min_confidence: f32,
}

impl DecisionEngine {
    pub fn new(bad_words: HashSet<String>, max_distance: u64, min_confidence: f32) -> Self {
        Self {
            rules: Rules::new(bad_words, max_distance),
            min_confidence,
        }
    }

    pub fn get_interval_ban_time(&self, timed_word: &TimedWord) -> Option<IntervalBanTime> {
        // Неуверенно распознанное слово — часто шум, «дорисованный» Vosk до похожего слова
        if timed_word.conf < self.min_confidence {
            return None;
        }

        if self.rules.is_bad_word(&timed_word.word) {
            Some(IntervalBanTime {
                start_time: timed_word.start_ms,
                end_time: timed_word.end_ms,
            })
        } else {
            None
        }
    }

    /// Запись словаря, под которую попало слово, с теми же правилами, что и
    /// `get_interval_ban_time`; None — слово не цензурируется
    pub fn matched_entry(&self, timed_word: &TimedWord) -> Option<&str> {
        if timed_word.conf < self.min_confidence {
            return None;
        }
        self.rules.matched_entry(&timed_word.word)
    }

    // Используется для тестирования и симуляции потока данных
    pub fn get_batch_ban_time(&self, timed_words: &[TimedWord]) -> Option<Vec<IntervalBanTime>> {
        let mut ban_times = Vec::new();
        for timed_word in timed_words {
            if let Some(ban_time) = self.get_interval_ban_time(timed_word) {
                ban_times.push(ban_time);
            }
        }
        if ban_times.is_empty() {
            None
        } else {
            Some(ban_times)
        }
    }

    pub fn replace_dictionary(&mut self, new_dict: HashSet<String>) {
        self.rules.replace_dictionary(new_dict);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntervalBanTime {
    pub start_time: u64,
    pub end_time: u64,
}

impl IntervalBanTime {
    pub fn new(start_time: u64, end_time: u64) -> Self {
        Self {
            start_time,
            end_time,
        }
    }
}
