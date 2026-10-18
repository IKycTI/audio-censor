use audio_censor::decision::{DetectedInterval, IntervalBanTime, PhraseOutcome, PhraseTracker};

fn iv(start: u64, end: u64) -> IntervalBanTime {
    IntervalBanTime::new(start, end)
}

fn new(start: u64, end: u64) -> DetectedInterval {
    DetectedInterval {
        interval: iv(start, end),
        is_new: true,
    }
}

fn update(start: u64, end: u64) -> DetectedInterval {
    DetectedInterval {
        interval: iv(start, end),
        is_new: false,
    }
}

// ── Частичные гипотезы ─────────────────────────────────────────────

#[test]
fn word_from_partial_is_new_once() {
    let mut t = PhraseTracker::new();
    assert_eq!(t.on_partial(vec![iv(1000, 1300)]), vec![new(1000, 1300)]);
    // Та же гипотеза на следующих чанках — ничего нового
    assert!(t.on_partial(vec![iv(1000, 1300)]).is_empty());
    assert!(t.on_partial(vec![iv(1010, 1290)]).is_empty());
}

#[test]
fn widened_word_is_sent_as_update() {
    let mut t = PhraseTracker::new();
    t.on_partial(vec![iv(1000, 1200)]);
    // Слово ещё звучало — конец уточнился
    assert_eq!(t.on_partial(vec![iv(1000, 1300)]), vec![update(1000, 1300)]);
    // Уточнение запомнено: повтор не даёт нового обновления
    assert!(t.on_partial(vec![iv(1000, 1300)]).is_empty());
}

#[test]
fn update_is_union_of_old_and_new_bounds() {
    let mut t = PhraseTracker::new();
    t.on_partial(vec![iv(1000, 1200)]);
    // Сдвиг влево: начало раньше, конец не дальше — маска не должна сужаться
    assert_eq!(t.on_partial(vec![iv(950, 1150)]), vec![update(950, 1200)]);
}

#[test]
fn second_word_in_phrase_is_new() {
    let mut t = PhraseTracker::new();
    t.on_partial(vec![iv(1000, 1300)]);
    assert_eq!(
        t.on_partial(vec![iv(1000, 1300), iv(1500, 1700)]),
        vec![new(1500, 1700)]
    );
}

// ── Финальный результат ────────────────────────────────────────────

#[test]
fn final_confirms_partial_without_resending() {
    let mut t = PhraseTracker::new();
    t.on_partial(vec![iv(1000, 1300)]);
    assert_eq!(t.on_final(vec![iv(1000, 1300)]), PhraseOutcome::default());
}

#[test]
fn final_with_wider_bounds_sends_update() {
    let mut t = PhraseTracker::new();
    t.on_partial(vec![iv(1000, 1250)]);
    let outcome = t.on_final(vec![iv(990, 1300)]);
    assert_eq!(outcome.detected, vec![update(990, 1300)]);
    assert_eq!(outcome.unconfirmed, 0);
}

#[test]
fn partial_word_missing_in_final_is_unconfirmed() {
    // «дурак» → «дурацкий»: в финале запрещённого слова нет
    let mut t = PhraseTracker::new();
    t.on_partial(vec![iv(1000, 1300), iv(2000, 2200)]);
    let outcome = t.on_final(vec![iv(2000, 2200)]);
    assert!(outcome.detected.is_empty());
    assert_eq!(outcome.unconfirmed, 1);
}

#[test]
fn word_only_in_final_is_new() {
    // Частичные гипотезы выключены или пропустили слово
    let mut t = PhraseTracker::new();
    let outcome = t.on_final(vec![iv(1000, 1300), iv(1500, 1700)]);
    assert_eq!(outcome.detected, vec![new(1000, 1300), new(1500, 1700)]);
    assert_eq!(outcome.unconfirmed, 0);
}

#[test]
fn final_resets_phrase() {
    let mut t = PhraseTracker::new();
    t.on_partial(vec![iv(1000, 1300)]);
    t.on_final(vec![iv(1000, 1300)]);

    // Следующая фраза: то же слово в другое время — снова новое,
    // а старое не считается неподтверждённым
    assert_eq!(t.on_partial(vec![iv(5000, 5300)]), vec![new(5000, 5300)]);
    let outcome = t.on_final(vec![iv(5000, 5300)]);
    assert_eq!(outcome.unconfirmed, 0);
}

#[test]
fn final_interval_spanning_two_partials_confirms_both() {
    let mut t = PhraseTracker::new();
    t.on_partial(vec![iv(1000, 1100), iv(1200, 1300)]);
    let outcome = t.on_final(vec![iv(1000, 1300)]);
    assert_eq!(outcome.unconfirmed, 0);
}
