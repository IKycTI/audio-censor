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
        let normalized_word = normalize_word(word);

        if self.bad_words.contains(&normalized_word) {
            return true;
        }

        // Короткие слова — только точное совпадение, и это проверяется с обеих сторон:
        // иначе короткое слово словаря ловит похожие длинные («три» ↔ «трио»)
        if self.max_distance == 0 || normalized_word.chars().count() < SHORTEST_WORD_LENGTH {
            return false;
        }

        self.bad_words
            .iter()
            .filter(|bad_word| bad_word.chars().count() >= SHORTEST_WORD_LENGTH)
            .any(|bad_word| levenshtein(&normalized_word, bad_word) <= self.max_distance)
    }

    pub fn replace_dictionary(&mut self, new_dict: HashSet<String>) {
        self.bad_words = normalize_dictionary(&new_dict);
    }
}

fn normalize_dictionary(words: &HashSet<String>) -> HashSet<String> {
    words.iter().map(|w| normalize_word(w)).collect()
}

fn normalize_word(word: &str) -> String {
    word.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric(), "")
}
