use distance::levenshtein;
use std::collections::HashSet;

// Ограничитель ложных срабатываний у Левенштейна
const SHORTEST_WORD_LENGTH: usize = 3;

pub struct Rules {
    pub bad_words: HashSet<String>,
    pub max_distance: usize,
}

impl Rules {
    pub fn new(bad_words: HashSet<String>, max_distance: u64) -> Self {
        Self {
            bad_words,
            max_distance: max_distance as usize,
        }
    }

    pub fn is_bad_word(&self, word: &str) -> bool {
        let normalized_word = normalize_word(word);

        for bad_word in self.bad_words.iter() {
            let normalized_bad_word = normalize_word(bad_word);
            if normalized_word == normalized_bad_word {
                return true;
            }

            if normalized_word.len() >= SHORTEST_WORD_LENGTH
                && levenshtein(&normalized_word, &normalized_bad_word) <= self.max_distance
            {
                return true;
            }
        }
        false
    }

    pub fn replace_dictionary(&mut self, new_dict: HashSet<String>) {
        self.bad_words = new_dict;
    }
}

fn normalize_word(word: &str) -> String {
    word.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric(), "")
}
