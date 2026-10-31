use distance::levenshtein;
use std::collections::HashSet;

// Ограничитель ложных срабатываний у Левенштейна
const SHORTEST_WORD_LENGTH: usize = 4;

pub struct Rules {
    pub bad_words: HashSet<String>,
    pub max_distance: usize,
}

impl Rules {
    pub fn new(bad_words: HashSet<String>, max_distance: u64) -> Self {
        Self {
            bad_words: normalize_dictionary(&bad_words),
            max_distance: max_distance as usize,
        }
    }

    pub fn is_bad_word(&self, word: &str) -> bool {
        self.matched_entry(word).is_some()
    }

    /// Запись словаря, под которую попало слово: точное совпадение или ближайшая
    /// по Левенштейну (при равенстве — первая по алфавиту, чтобы метрики не зависели
    /// от порядка обхода HashSet)
    pub fn matched_entry(&self, word: &str) -> Option<&str> {
        let normalized_word = normalize_word(word);

        if let Some(entry) = self.bad_words.get(&normalized_word) {
            return Some(entry);
        }

        // Короткие слова — только точное совпадение, и это проверяется с обеих сторон:
        // иначе короткое слово словаря ловит похожие длинные («три» ↔ «трио»)
        if self.max_distance == 0 || normalized_word.chars().count() < SHORTEST_WORD_LENGTH {
            return None;
        }

        self.bad_words
            .iter()
            .filter(|bad_word| bad_word.chars().count() >= SHORTEST_WORD_LENGTH)
            .map(|bad_word| (levenshtein(&normalized_word, bad_word), bad_word))
            .filter(|(distance, _)| *distance <= self.max_distance)
            .min()
            .map(|(_, bad_word)| bad_word.as_str())
    }

    pub fn replace_dictionary(&mut self, new_dict: HashSet<String>) {
        self.bad_words = normalize_dictionary(&new_dict);
    }
}

fn normalize_dictionary(words: &HashSet<String>) -> HashSet<String> {
    words.iter().map(|w| normalize_word(w)).collect()
}

/// Нижний регистр, только буквы и цифры — так сравниваются и слова, и записи словаря
pub fn normalize_word(word: &str) -> String {
    word.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric(), "")
}
