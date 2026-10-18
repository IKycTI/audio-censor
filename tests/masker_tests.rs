use std::f32::consts::PI;

use audio_censor::decision::IntervalBanTime;
use audio_censor::masking::{Beep, IntervalTiming, Masker, Masking, MuteAttenuate, Silence};
use audio_censor::ms_to_stream_pos;

/// Сэмпл-рейт 1000 Гц, моно → 1 мс = 1 позиция, фейд (5 мс) = 5 позиций.
/// Интервал [10, 40] без паддинга: фейд-ин на 10..=14, фейд-аут на 36..=40,
/// «чистая» маска — 15..=35.
const SAMPLE_RATE: u32 = 1000;
const MONO: u16 = 1;
const ORIG: f32 = 0.8;
const EPS: f32 = 1e-5;

fn masker(strategy: Box<dyn Masking<f32>>, padding_ms: u64) -> Masker<f32> {
    Masker::new(strategy, padding_ms, SAMPLE_RATE, MONO)
}

/// Батч позиций [from, to) с постоянным значением.
fn batch(from: u64, to: u64, value: f32) -> Vec<(u64, f32)> {
    (from..to).map(|pos| (pos, value)).collect()
}

/// Значение выхода по абсолютной позиции, если батч начинается с `from`.
fn at(output: &[f32], from: u64, pos: u64) -> f32 {
    output[(pos - from) as usize]
}

fn assert_close(actual: f32, expected: f32, pos: u64) {
    assert!(
        (actual - expected).abs() < EPS,
        "позиция {pos}: ожидалось {expected}, получено {actual}"
    );
}

/// Значение бипа для кадра `frame` от начала интервала.
fn beep_value(frequency_hz: f32, amplitude: f32, frame: u64, sample_rate: u32) -> f32 {
    amplitude * (2.0 * PI * frequency_hz * frame as f32 / sample_rate as f32).sin()
}

// ── Без интервалов ─────────────────────────────────────────────────

#[test]
fn passthrough_without_intervals() {
    let mut m = masker(Box::new(Silence::new()), 0);
    let input: Vec<(u64, f32)> = (0..50).map(|p| (p, p as f32 * 0.01)).collect();

    let output = m.process(&input);

    let expected: Vec<f32> = input.iter().map(|&(_, s)| s).collect();
    assert_eq!(output, expected);
}

#[test]
fn empty_input_gives_empty_output() {
    let mut m = masker(Box::new(Silence::new()), 0);
    m.add_interval(&IntervalBanTime::new(10, 40));
    assert!(m.process(&[]).is_empty());
}

#[test]
fn output_len_equals_input_len() {
    let mut m = masker(Box::new(Silence::new()), 0);
    m.add_interval(&IntervalBanTime::new(10, 40));
    let input = batch(0, 60, ORIG);
    assert_eq!(m.process(&input).len(), input.len());
}

// ── Silence ────────────────────────────────────────────────────────

#[test]
fn silence_masks_interval_and_keeps_rest() {
    let mut m = masker(Box::new(Silence::new()), 0);
    m.add_interval(&IntervalBanTime::new(10, 40));

    let output = m.process(&batch(0, 60, ORIG));

    for pos in 0..10 {
        assert_close(at(&output, 0, pos), ORIG, pos);
    }
    for pos in 15..=35 {
        assert_close(at(&output, 0, pos), 0.0, pos);
    }
    for pos in 41..60 {
        assert_close(at(&output, 0, pos), ORIG, pos);
    }
}

#[test]
fn fades_are_monotonic_at_mask_edges() {
    let mut m = masker(Box::new(Silence::new()), 0);
    m.add_interval(&IntervalBanTime::new(10, 40));

    let output = m.process(&batch(0, 60, ORIG));

    // Вход в маску: уровень не растёт до полной тишины
    for pos in 9..15 {
        assert!(
            at(&output, 0, pos + 1) <= at(&output, 0, pos) + EPS,
            "фейд-ин не монотонен на позиции {pos}"
        );
    }
    // Выход из маски: уровень не падает до исходного
    for pos in 35..41 {
        assert!(
            at(&output, 0, pos + 1) + EPS >= at(&output, 0, pos),
            "фейд-аут не монотонен на позиции {pos}"
        );
    }
    // Фейд действительно сглаживает, а не обрывает
    assert!(at(&output, 0, 12) > 0.0 && at(&output, 0, 12) < ORIG);
    assert!(at(&output, 0, 38) > 0.0 && at(&output, 0, 38) < ORIG);
}

// ── Паддинг ────────────────────────────────────────────────────────

#[test]
fn padding_extends_interval_on_both_sides() {
    // padding 3 мс = 3 позиции → фактический интервал [7, 43]
    let mut m = masker(Box::new(Silence::new()), 3);
    m.add_interval(&IntervalBanTime::new(10, 40));

    let output = m.process(&batch(0, 60, ORIG));

    assert_close(at(&output, 0, 6), ORIG, 6);
    assert!(
        at(&output, 0, 8) < ORIG - EPS,
        "позиция 8 должна попасть в маску"
    );
    assert!(
        at(&output, 0, 42) < ORIG - EPS,
        "позиция 42 должна попасть в маску"
    );
    assert_close(at(&output, 0, 44), ORIG, 44);
}

#[test]
fn padding_does_not_underflow_at_stream_start() {
    let mut m = masker(Box::new(Silence::new()), 5);
    m.add_interval(&IntervalBanTime::new(1, 20)); // начало 1 - 5 → 0, а не переполнение

    let output = m.process(&batch(0, 40, ORIG));

    assert_close(at(&output, 0, 10), 0.0, 10);
    assert_close(at(&output, 0, 35), ORIG, 35);
}

// ── Несколько интервалов ───────────────────────────────────────────

#[test]
fn multiple_disjoint_intervals() {
    let mut m = masker(Box::new(Silence::new()), 0);
    m.add_interval(&IntervalBanTime::new(10, 30));
    m.add_interval(&IntervalBanTime::new(50, 70));

    let output = m.process(&batch(0, 80, ORIG));

    assert_close(at(&output, 0, 5), ORIG, 5);
    assert_close(at(&output, 0, 20), 0.0, 20);
    assert_close(at(&output, 0, 40), ORIG, 40);
    assert_close(at(&output, 0, 60), 0.0, 60);
    assert_close(at(&output, 0, 75), ORIG, 75);
}

#[test]
fn overlapping_intervals_mask_their_union() {
    let mut m = masker(Box::new(Silence::new()), 0);
    m.add_interval(&IntervalBanTime::new(10, 30));
    m.add_interval(&IntervalBanTime::new(20, 40));

    let output = m.process(&batch(0, 60, ORIG));

    // Стык интервалов (30 → 31) не должен давать фейд
    for pos in 15..=35 {
        assert_close(at(&output, 0, pos), 0.0, pos);
    }
    assert_close(at(&output, 0, 45), ORIG, 45);
}

// ── Порядок поступления интервалов ─────────────────────────────────

#[test]
fn interval_registered_mid_stream_masks_remaining_part() {
    let mut m = masker(Box::new(Silence::new()), 0);

    // Первые 10 позиций ушли до того, как интервал стал известен
    let first = m.process(&batch(0, 10, ORIG));
    assert!(first.iter().all(|&s| (s - ORIG).abs() < EPS));

    m.add_interval(&IntervalBanTime::new(5, 40));
    let output = m.process(&batch(10, 60, ORIG));

    for pos in 15..=35 {
        assert_close(at(&output, 10, pos), 0.0, pos);
    }
    for pos in 41..60 {
        assert_close(at(&output, 10, pos), ORIG, pos);
    }
}

#[test]
fn late_interval_does_not_affect_future_samples() {
    let mut m = masker(Box::new(Silence::new()), 0);
    m.process(&batch(0, 50, ORIG));

    // Интервал целиком в прошлом — опоздал
    m.add_interval(&IntervalBanTime::new(10, 20));
    let output = m.process(&batch(50, 100, ORIG));

    assert!(output.iter().all(|&s| (s - ORIG).abs() < EPS));
}

// ── Другие стратегии через Masker ──────────────────────────────────

#[test]
fn mute_attenuate_scales_masked_samples() {
    // -20 дБ → множитель 0.1
    let mut m = masker(Box::new(MuteAttenuate::new(-20.0)), 0);
    m.add_interval(&IntervalBanTime::new(10, 40));

    let output = m.process(&batch(0, 60, ORIG));

    for pos in 15..=35 {
        assert_close(at(&output, 0, pos), ORIG * 0.1, pos);
    }
    assert_close(at(&output, 0, 50), ORIG, 50);
}

#[test]
fn beep_phase_starts_at_interval_start() {
    let (freq, amp) = (100.0, 0.5);
    let mut m = masker(Box::new(Beep::new(freq, amp)), 0);
    m.add_interval(&IntervalBanTime::new(10, 40));

    let output = m.process(&batch(0, 60, ORIG));

    for pos in 15..=35 {
        assert_close(
            at(&output, 0, pos),
            beep_value(freq, amp, pos - 10, SAMPLE_RATE),
            pos,
        );
    }
}

#[test]
fn beep_phase_continues_across_batches() {
    let (freq, amp) = (100.0, 0.5);
    let mut m = masker(Box::new(Beep::new(freq, amp)), 0);
    m.add_interval(&IntervalBanTime::new(10, 40));

    m.process(&batch(0, 25, ORIG));
    let second = m.process(&batch(25, 60, ORIG));

    // Позиции вдали от краёв батча и интервала: фаза продолжается, а не сбрасывается
    for pos in 30..=35 {
        assert_close(
            at(&second, 25, pos),
            beep_value(freq, amp, pos - 10, SAMPLE_RATE),
            pos,
        );
    }
}

// ── Стерео 44100 Гц: мс → позиция в потоке ─────────────────────────

const CD_RATE: u32 = 44100;
const STEREO: u16 = 2;

#[test]
fn ms_to_stream_pos_counts_interleaved_samples() {
    assert_eq!(ms_to_stream_pos(3000, CD_RATE, STEREO), 264600);
    assert_eq!(ms_to_stream_pos(3200, CD_RATE, STEREO), 282240);
    assert_eq!(ms_to_stream_pos(3000, CD_RATE, 1), 132300);
}

#[test]
fn ms_to_stream_pos_is_aligned_to_frame() {
    // 1 мс = 44.1 кадра → округление вниз до 44 кадров, а не 88.2 сэмпла
    assert_eq!(ms_to_stream_pos(1, CD_RATE, STEREO), 88);
    for ms in 0..100 {
        assert_eq!(ms_to_stream_pos(ms, CD_RATE, STEREO) % STEREO as u64, 0);
    }
}

#[test]
fn stereo_interval_in_ms_maps_to_stream_positions() {
    // Слово 3000–3200 мс ↔ позиции 264600..=282241 (конец дополнен до границы кадра)
    let mut m: Masker<f32> = Masker::new(Box::new(Silence::new()), 0, CD_RATE, STEREO);
    m.add_interval(&IntervalBanTime::new(3000, 3200));

    let from = 264000;
    let output = m.process(&batch(from, 283000, ORIG));

    // Фейд 5 мс = 220 кадров = 440 позиций
    let fade = ms_to_stream_pos(5, CD_RATE, STEREO);
    assert_close(at(&output, from, 264599), ORIG, 264599);
    for pos in (264600 + fade)..=(282241 - fade) {
        assert_close(at(&output, from, pos), 0.0, pos);
    }
    assert_close(at(&output, from, 282242), ORIG, 282242);
}

#[test]
fn stereo_padding_is_counted_in_frames() {
    // padding 10 мс = 441 кадр = 882 позиции с каждой стороны
    let mut m: Masker<f32> = Masker::new(Box::new(Silence::new()), 10, CD_RATE, STEREO);
    m.add_interval(&IntervalBanTime::new(3000, 3200));

    let from = 262000;
    let output = m.process(&batch(from, 284000, ORIG));

    // Крайние кадры маски — граница фейда (t = 0), поэтому проверяем соседние с ними
    assert_close(at(&output, from, 264600 - 882 - 1), ORIG, 264600 - 882 - 1);
    assert!(at(&output, from, 264600 - 882 + 2) < ORIG - EPS);
    assert!(at(&output, from, 282241 + 882 - 2) < ORIG - EPS);
    assert_close(at(&output, from, 282241 + 882 + 1), ORIG, 282241 + 882 + 1);
}

#[test]
fn stereo_beep_has_same_value_in_both_channels_of_frame() {
    let (freq, amp) = (1000.0, 0.5);
    let mut m: Masker<f32> = Masker::new(Box::new(Beep::new(freq, amp)), 0, CD_RATE, STEREO);
    m.add_interval(&IntervalBanTime::new(3000, 3200));

    let from = 264000;
    let output = m.process(&batch(from, 283000, ORIG));

    // Вне фейдов: оба канала кадра равны, частота считается по кадрам
    for pos in (265100..281700).step_by(2) {
        let frame = (pos - 264600) / STEREO as u64;
        let expected = beep_value(freq, amp, frame, CD_RATE);
        assert_close(at(&output, from, pos), expected, pos);
        assert_close(at(&output, from, pos + 1), expected, pos + 1);
    }
}

#[test]
fn stereo_beep_phase_is_relative_to_interval_start_when_late() {
    let (freq, amp) = (1000.0, 0.5);
    let mut m: Masker<f32> = Masker::new(Box::new(Beep::new(freq, amp)), 0, CD_RATE, STEREO);

    // Батч обрывается посреди кадра: 265000 — левый канал, 265001 — правый
    let first_end = 265001;
    m.process(&batch(264000, first_end, ORIG));

    // Интервал [264600, 282241] стал известен, когда его начало уже ушло
    m.add_interval(&IntervalBanTime::new(3000, 3200));
    let output = m.process(&batch(first_end, 283000, ORIG));

    // Вне фейда на краю батча: каналы кадра равны, фаза — от начала интервала
    for pos in (265500..281700).step_by(2) {
        let frame = (pos - 264600) / STEREO as u64;
        let expected = beep_value(freq, amp, frame, CD_RATE);
        assert_close(at(&output, first_end, pos), expected, pos);
        assert_close(at(&output, first_end, pos + 1), expected, pos + 1);
    }
}

// ── Опоздание интервала (метрика late_intervals) ───────────────────

#[test]
fn interval_before_playback_is_on_time() {
    let mut m = masker(Box::new(Silence::new()), 0);
    assert_eq!(
        m.add_interval(&IntervalBanTime::new(10, 40)),
        IntervalTiming::OnTime
    );

    m.process(&batch(0, 10, ORIG));
    // Позиция 10 ещё не выдана — слово целиком впереди
    assert_eq!(
        m.add_interval(&IntervalBanTime::new(10, 40)),
        IntervalTiming::OnTime
    );
}

#[test]
fn interval_with_started_word_is_partially_late() {
    let mut m = masker(Box::new(Silence::new()), 0);
    m.process(&batch(0, 11, ORIG));

    // Позиция 10 — первый сэмпл слова — уже ушла на выход
    let timing = m.add_interval(&IntervalBanTime::new(10, 40));
    assert_eq!(timing, IntervalTiming::PartiallyLate);
    assert!(timing.is_late());
}

#[test]
fn interval_after_word_is_late() {
    let mut m = masker(Box::new(Silence::new()), 0);
    m.process(&batch(0, 41, ORIG));

    let timing = m.add_interval(&IntervalBanTime::new(10, 40));
    assert_eq!(timing, IntervalTiming::Late);
    assert!(timing.is_late());
}

#[test]
fn lost_padding_does_not_count_as_late() {
    // padding 5 → маска [5, 45], но слово [10, 40] ещё не звучало
    let mut m = masker(Box::new(Silence::new()), 5);
    m.process(&batch(0, 8, ORIG));

    assert_eq!(
        m.add_interval(&IntervalBanTime::new(10, 40)),
        IntervalTiming::OnTime
    );
}

#[test]
fn stereo_last_frame_of_word_decides_late() {
    // Слово 3000–3200 мс ↔ позиции 264600..=282241
    let mut m: Masker<f32> = Masker::new(Box::new(Silence::new()), 0, CD_RATE, STEREO);

    // Выдан только левый канал последнего кадра — правый ещё можно заглушить
    m.process(&batch(264000, 282241, ORIG));
    assert_eq!(
        m.add_interval(&IntervalBanTime::new(3000, 3200)),
        IntervalTiming::PartiallyLate
    );

    let mut m: Masker<f32> = Masker::new(Box::new(Silence::new()), 0, CD_RATE, STEREO);
    m.process(&batch(264000, 282242, ORIG));
    assert_eq!(
        m.add_interval(&IntervalBanTime::new(3000, 3200)),
        IntervalTiming::Late
    );
}

#[test]
fn output_position_ms_tracks_processed_samples() {
    let mut m: Masker<f32> = Masker::new(Box::new(Silence::new()), 0, CD_RATE, STEREO);
    assert_eq!(m.output_position_ms(), 0);
    // 3000 мс стерео 44100 Гц = позиции 0..264600
    m.process(&batch(0, 264600, ORIG));
    assert_eq!(m.output_position_ms(), 3000);
}

// ── Известные ошибки (см. ERROR.md) ────────────────────────────────

#[test]
fn mask_is_continuous_across_batches() {
    let mut m = masker(Box::new(Silence::new()), 0);
    m.add_interval(&IntervalBanTime::new(10, 90));

    // Маска тянется через 10 батчей по 10 позиций, как в реальном цикле оркестратора
    let mut output = Vec::new();
    for from in (0..100).step_by(10) {
        output.extend(m.process(&batch(from, from + 10, ORIG)));
    }

    for pos in 15..=85 {
        assert_close(at(&output, 0, pos), 0.0, pos);
    }
}

#[test]
fn output_does_not_depend_on_batch_split() {
    // Фейды и маска режутся батчами по 7 позиций — результат как у одного батча
    let mut whole = masker(Box::new(Beep::new(100.0, 0.5)), 0);
    whole.add_interval(&IntervalBanTime::new(10, 90));
    let expected = whole.process(&batch(0, 100, ORIG));

    let mut split = masker(Box::new(Beep::new(100.0, 0.5)), 0);
    split.add_interval(&IntervalBanTime::new(10, 90));
    let mut output = Vec::new();
    for from in (0..100).step_by(7) {
        output.extend(split.process(&batch(from, (from + 7).min(100), ORIG)));
    }

    for (pos, (a, b)) in expected.iter().zip(&output).enumerate() {
        assert_close(*b, *a, pos as u64);
    }
}

#[test]
fn stereo_fade_has_same_value_in_both_channels_of_frame() {
    let mut m: Masker<f32> = Masker::new(Box::new(Silence::new()), 0, CD_RATE, STEREO);
    m.add_interval(&IntervalBanTime::new(3000, 3200));

    let from = 264000;
    let output = m.process(&batch(from, 283000, ORIG));

    // Фейд-ин и фейд-аут: левый и правый канал кадра смешиваются одинаково
    let fade = ms_to_stream_pos(5, CD_RATE, STEREO);
    for pos in (264600..264600 + fade)
        .chain(282242 - fade..282242)
        .step_by(2)
    {
        assert_close(at(&output, from, pos + 1), at(&output, from, pos), pos + 1);
    }
}

#[test]
fn adjacent_intervals_merge_without_fade_at_joint() {
    let mut m = masker(Box::new(Silence::new()), 0);
    m.add_interval(&IntervalBanTime::new(10, 30));
    m.add_interval(&IntervalBanTime::new(31, 50));

    let output = m.process(&batch(0, 60, ORIG));

    for pos in 15..=45 {
        assert_close(at(&output, 0, pos), 0.0, pos);
    }
}

#[test]
fn late_interval_fades_in_from_current_position() {
    let mut m = masker(Box::new(Silence::new()), 0);
    m.process(&batch(0, 20, ORIG));

    // Начало [10, 60] уже ушло: маска плавно включается с позиции 20, а не скачком
    m.add_interval(&IntervalBanTime::new(10, 60));
    let output = m.process(&batch(20, 70, ORIG));

    assert_close(at(&output, 20, 20), ORIG, 20);
    assert!(at(&output, 20, 22) > 0.0 && at(&output, 20, 22) < ORIG);
    for pos in 25..=55 {
        assert_close(at(&output, 20, pos), 0.0, pos);
    }
}

#[test]
fn beep_phase_survives_merge_with_earlier_interval() {
    let (freq, amp) = (100.0, 0.5);
    let mut m = masker(Box::new(Beep::new(freq, amp)), 0);
    m.add_interval(&IntervalBanTime::new(30, 80));
    m.process(&batch(0, 50, ORIG));

    // Опоздавшее слово [13, 60] сливается со звучащим [30, 80] и сдвигает start с 30 до 13
    // (сдвиг 17 не кратен периоду бипа в 10 позиций),
    // но фаза бипа продолжает отсчёт от 30
    m.add_interval(&IntervalBanTime::new(13, 60));
    let output = m.process(&batch(50, 100, ORIG));

    for pos in 50..=75 {
        assert_close(
            at(&output, 50, pos),
            beep_value(freq, amp, pos - 30, SAMPLE_RATE),
            pos,
        );
    }
}

#[test]
fn beep_phase_restarts_from_merged_start_before_playback() {
    let (freq, amp) = (100.0, 0.5);
    let mut m = masker(Box::new(Beep::new(freq, amp)), 0);

    // Оба интервала пришли до того, как маска зазвучала: фаза — от общего начала
    m.add_interval(&IntervalBanTime::new(30, 80));
    m.add_interval(&IntervalBanTime::new(13, 60));
    let output = m.process(&batch(0, 100, ORIG));

    for pos in 18..=75 {
        assert_close(
            at(&output, 0, pos),
            beep_value(freq, amp, pos - 13, SAMPLE_RATE),
            pos,
        );
    }
}

#[test]
fn silence_uses_equilibrium_for_unsigned() {
    let mut m: Masker<u16> = Masker::new(Box::new(Silence::new()), 0, SAMPLE_RATE, MONO);
    m.add_interval(&IntervalBanTime::new(10, 40));

    let input: Vec<(u64, u16)> = (0..60).map(|pos| (pos, 40000)).collect();
    let output = m.process(&input);

    for (i, &sample) in output[15..=35].iter().enumerate() {
        let pos = 15 + i;
        assert_eq!(
            sample, 32768,
            "позиция {pos}: тишина для u16 — 32768, а не 0"
        );
    }
}

// ── Стратегии напрямую ─────────────────────────────────────────────

#[test]
fn silence_strategy_keeps_length() {
    let out: Vec<f32> = Silence::new().apply(&[0.5; 17], SAMPLE_RATE, 0, MONO);
    assert_eq!(out, vec![0.0; 17]);
}

#[test]
fn beep_strategy_offset_continues_phase() {
    let beep = Beep::new(100.0, 0.5);
    let whole: Vec<f32> = beep.apply(&[0.0; 20], SAMPLE_RATE, 0, MONO);

    let mut split: Vec<f32> = beep.apply(&[0.0; 10], SAMPLE_RATE, 0, MONO);
    split.extend::<Vec<f32>>(beep.apply(&[0.0; 10], SAMPLE_RATE, 10, MONO));

    for (i, (a, b)) in whole.iter().zip(&split).enumerate() {
        assert_close(*b, *a, i as u64);
    }
}

#[test]
fn beep_strategy_stereo_matches_mono_per_frame() {
    let beep = Beep::new(100.0, 0.5);
    let mono: Vec<f32> = beep.apply(&[0.0; 10], SAMPLE_RATE, 0, MONO);
    let stereo: Vec<f32> = beep.apply(&[0.0; 20], SAMPLE_RATE, 0, STEREO);

    for (frame, expected) in mono.iter().enumerate() {
        assert_close(stereo[2 * frame], *expected, 2 * frame as u64);
        assert_close(stereo[2 * frame + 1], *expected, 2 * frame as u64 + 1);
    }
}

#[test]
fn beep_strategy_clamps_amplitude() {
    let out: Vec<f32> = Beep::new(100.0, 10.0).apply(&[0.0; 50], SAMPLE_RATE, 0, MONO);
    assert!(out.iter().all(|s| (-1.0..=1.0).contains(s)));
}

#[test]
fn mute_attenuate_zero_db_is_identity() {
    let input = [0.1, -0.5, 0.9];
    let out: Vec<f32> = MuteAttenuate::new(0.0).apply(&input, SAMPLE_RATE, 0, MONO);
    for (i, (a, b)) in input.iter().zip(&out).enumerate() {
        assert_close(*b, *a, i as u64);
    }
}
