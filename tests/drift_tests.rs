use audio_censor::pipeline::{DriftCompensator, DriftCorrection};

/// 1000 Гц моно → 1 мс = 1 сэмпл. Цель 200 мс = 200, допуск ±100,
/// коррекция не чаще раза в 20 кадров
const SAMPLE_RATE: u32 = 1000;
const TARGET_MS: u64 = 200;
const SILENCE: f32 = 0.0;

fn batch(len: usize) -> Vec<f32> {
    (0..len).map(|i| 0.1 + i as f32 * 0.001).collect()
}

/// Компенсатор после старта: первый батч уже дополнен до цели
fn primed(channels: u16) -> DriftCompensator {
    let mut drift = DriftCompensator::new(TARGET_MS, SAMPLE_RATE, channels);
    let mut b = batch(10 * channels as usize);
    drift.adjust(&mut b, 0, SILENCE);
    drift
}

/// Подать много батчей при постоянной заполненности и вернуть все коррекции
fn feed(
    drift: &mut DriftCompensator,
    fill: usize,
    batches: usize,
    len: usize,
) -> Vec<DriftCorrection> {
    (0..batches)
        .map(|_| {
            let mut b = batch(len);
            drift.adjust(&mut b, fill, SILENCE)
        })
        .collect()
}

/// Подавать батчи, пока не случится коррекция `want`, и вернуть подправленный батч
fn until_correction(
    drift: &mut DriftCompensator,
    fill: usize,
    original: &[f32],
    want: DriftCorrection,
) -> Vec<f32> {
    for _ in 0..10_000 {
        let mut b = original.to_vec();
        if drift.adjust(&mut b, fill, SILENCE) == want {
            return b;
        }
    }
    panic!("коррекция {want:?} так и не случилась");
}

// ── Старт и срыв воспроизведения ───────────────────────────────────

#[test]
fn first_batch_is_prefilled_with_silence_to_target() {
    let mut drift = DriftCompensator::new(TARGET_MS, SAMPLE_RATE, 1);
    let mut b = batch(10);
    let original = b.clone();

    assert_eq!(
        drift.adjust(&mut b, 0, SILENCE),
        DriftCorrection::Prefilled(190)
    );
    assert_eq!(b.len(), 200);
    // Тишина — в начале, сам звук не тронут и идёт следом
    assert!(b[..190].iter().all(|&s| s == SILENCE));
    assert_eq!(&b[190..], &original[..]);
}

#[test]
fn empty_output_buffer_is_refilled_at_once() {
    let mut drift = primed(1);
    let mut b = batch(10);
    // Буфер опустел (срыв воспроизведения) — дотягивать по кадру слишком долго
    assert_eq!(
        drift.adjust(&mut b, 0, SILENCE),
        DriftCorrection::Prefilled(190)
    );
}

#[test]
fn prefill_is_whole_frames_in_stereo() {
    let mut drift = DriftCompensator::new(TARGET_MS, SAMPLE_RATE, 2);
    let mut b = batch(20);
    assert_eq!(
        drift.adjust(&mut b, 0, SILENCE),
        DriftCorrection::Prefilled(190)
    );
    assert_eq!(b.len(), 400);
}

// ── Устойчивое состояние ───────────────────────────────────────────

#[test]
fn no_correction_within_band() {
    let mut drift = primed(1);
    // После записи батча в буфере 200 ± 90 — в пределах допуска
    for fill in [100, 190, 280] {
        let corrections = feed(&mut drift, fill, 500, 10);
        assert!(
            corrections.iter().all(|&c| c == DriftCorrection::None),
            "fill {fill}: коррекций быть не должно"
        );
    }
}

#[test]
fn overfilled_buffer_drops_one_frame() {
    // Вход быстрее выхода — буфер растёт
    let mut drift = primed(1);
    let corrections = feed(&mut drift, 400, 500, 10);
    assert!(corrections.contains(&DriftCorrection::DroppedFrame));
    assert!(!corrections.contains(&DriftCorrection::DuplicatedFrame));

    // Выброшен ровно последний кадр
    let original = batch(10);
    let b = until_correction(&mut drift, 400, &original, DriftCorrection::DroppedFrame);
    assert_eq!(b, original[..9]);
}

#[test]
fn underfilled_buffer_duplicates_last_frame() {
    // Вход медленнее выхода — буфер пустеет
    let mut drift = primed(1);
    let corrections = feed(&mut drift, 50, 500, 10);
    assert!(!corrections.contains(&DriftCorrection::DroppedFrame));

    let original = batch(10);
    let b = until_correction(&mut drift, 50, &original, DriftCorrection::DuplicatedFrame);
    assert_eq!(b.len(), 11);
    assert_eq!(b[10], original[9]);
}

#[test]
fn stereo_correction_affects_whole_frame() {
    let mut drift = primed(2);
    feed(&mut drift, 100, 500, 20);

    let original = batch(20);
    let b = until_correction(&mut drift, 100, &original, DriftCorrection::DuplicatedFrame);
    // Дублируется кадр целиком: оба канала, в правильном порядке
    assert_eq!(b.len(), 22);
    assert_eq!(&b[20..], &original[18..]);
}

#[test]
fn corrections_are_rate_limited() {
    let mut drift = primed(1);
    // 1000 батчей по 10 кадров = 10 000 кадров; не чаще раза в 20 кадров → не больше 500
    let corrections = feed(&mut drift, 400, 1000, 10);
    let count = corrections
        .iter()
        .filter(|&&c| c == DriftCorrection::DroppedFrame)
        .count();
    assert!(count > 0 && count <= 500, "коррекций: {count}");
}

#[test]
fn smoothing_ignores_short_spikes() {
    let mut drift = primed(1);
    feed(&mut drift, 190, 500, 10);
    // Единичный всплеск заполненности (колбэк ещё не забрал данные) — не повод выбрасывать
    let corrections = feed(&mut drift, 2000, 3, 10);
    assert!(corrections.iter().all(|&c| c == DriftCorrection::None));
}

#[test]
fn empty_batch_is_left_alone() {
    let mut drift = DriftCompensator::new(TARGET_MS, SAMPLE_RATE, 2);
    let mut b: Vec<f32> = Vec::new();
    assert_eq!(drift.adjust(&mut b, 0, SILENCE), DriftCorrection::None);
    assert!(b.is_empty());
}
