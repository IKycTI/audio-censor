use audio_censor::decision::engine::{DecisionEngine, IntervalBanTime};
use audio_censor::recognition::events::TimedWord;
use std::collections::HashSet;

fn make_engine(words: &[&'static str], max_distance: usize) -> DecisionEngine {
    let bad_words: HashSet<&'static str> = words.iter().copied().collect();
    DecisionEngine::new(bad_words, max_distance)
}

#[test]
fn empty_input_returns_none() {
    let engine = make_engine(&["дурак"], 1);
    assert!(engine.get_batch_ban_time(&[]).is_none());
}

#[test]
fn no_banned_words_returns_none() {
    let engine = make_engine(&["дурак"], 1);
    let words = vec![
        TimedWord::new("привет", 0, 300),
        TimedWord::new("мир", 300, 600),
    ];
    assert!(engine.get_batch_ban_time(&words).is_none());
}

#[test]
fn exact_match_returns_interval() {
    let engine = make_engine(&["дурак"], 1);
    let words = vec![TimedWord::new("дурак", 100, 500)];
    let result = engine
        .get_batch_ban_time(&words)
        .expect("должен найти совпадение");
    assert_eq!(result, vec![IntervalBanTime::new(100, 500)]);
}

#[test]
fn interval_matches_word_boundaries_exactly_no_padding() {
    let engine = make_engine(&["дурак"], 0);
    let words = vec![TimedWord::new("дурак", 1234, 1789)];
    let result = engine.get_batch_ban_time(&words).unwrap();
    assert_eq!(result[0].start_time, 1234);
    assert_eq!(result[0].end_time, 1789);
}

#[test]
fn levenshtein_within_distance_is_banned() {
    let engine = make_engine(&["дурак"], 1);
    let words = vec![TimedWord::new("дурок", 0, 400)];
    assert!(engine.get_batch_ban_time(&words).is_some());
}

#[test]
fn levenshtein_beyond_distance_is_not_banned() {
    let engine = make_engine(&["дурак"], 1);
    let words = vec![TimedWord::new("дом", 0, 400)];
    assert!(engine.get_batch_ban_time(&words).is_none());
}

#[test]
fn exact_match_takes_priority_over_levenshtein_when_both_possible() {
    let engine = make_engine(&["кот", "дурак"], 2);
    let words = vec![TimedWord::new("кот", 0, 200)];
    let result = engine.get_batch_ban_time(&words).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn case_insensitive_matching() {
    let engine = make_engine(&["дурак"], 0);
    let words = vec![TimedWord::new("ДУРАК", 0, 300)];
    assert!(engine.get_batch_ban_time(&words).is_some());
}

#[test]
fn multiple_banned_words_produce_multiple_intervals_in_order() {
    let engine = make_engine(&["дурак", "болван"], 0);
    let words = vec![
        TimedWord::new("привет", 0, 200),
        TimedWord::new("дурак", 200, 500),
        TimedWord::new("мир", 500, 700),
        TimedWord::new("болван", 700, 1000),
    ];
    let result = engine.get_batch_ban_time(&words).unwrap();
    assert_eq!(
        result,
        vec![
            IntervalBanTime::new(200, 500),
            IntervalBanTime::new(700, 1000),
        ]
    );
}

#[test]
fn confidence_field_does_not_affect_decision() {
    let engine = make_engine(&["дурак"], 0);
    let low_conf_word = TimedWord::new("дурак", 0, 300);
    assert!(engine.get_batch_ban_time(&[low_conf_word]).is_some());
}

#[test]
fn max_distance_zero_behaves_as_exact_match_only() {
    let engine = make_engine(&["дурак"], 0);
    let words = vec![TimedWord::new("дурок", 0, 300)];
    assert!(engine.get_batch_ban_time(&words).is_none());
}
