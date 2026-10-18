//! Сквозной тест конвейера без Vosk и аудиоустройств: готовые `TimedWord`
//! → `DecisionEngine` → `Masker` поверх `DelayBuffer`, как в цикле оркестратора.

use std::collections::HashSet;

use audio_censor::buffer::DelayBuffer;
use audio_censor::decision::engine::DecisionEngine;
use audio_censor::masking::{IntervalTiming, Masker, Silence};
use audio_censor::ms_to_stream_pos;
use audio_censor::recognition::events::TimedWord;

/// 1000 Гц стерео: 1 мс = 1 кадр = 2 позиции
const SAMPLE_RATE: u32 = 1000;
const CHANNELS: u16 = 2;
const DELAY_MS: u64 = 500;
const BATCH_MS: u64 = 10;
const STREAM_MS: u64 = 2500;
const FADE_MS: u64 = 5;
const ORIG: f32 = 0.5;
const EPS: f32 = 1e-5;

/// Слова фразы: (слово, начало мс, конец мс)
const WORDS: [(&str, u64, u64); 4] = [
    ("привет", 100, 400),
    ("дурак", 1000, 1300),
    ("мир", 1400, 1550),
    ("Болван!", 1800, 2100),
];

struct Run {
    output: Vec<f32>,
    timings: Vec<IntervalTiming>,
}

/// Прогнать поток через конвейер. Слово «распознаётся» через `recognition_latency_ms`
/// после своего конца — так ведёт себя Vosk, отдающий слова по завершении фразы
fn run(recognition_latency_ms: u64) -> Run {
    let dict: HashSet<String> = ["дурак", "болван"].iter().map(|w| w.to_string()).collect();
    let engine = DecisionEngine::new(dict, 1, 0.0);
    let mut delay = DelayBuffer::new(DELAY_MS, SAMPLE_RATE, CHANNELS);
    let mut masker: Masker<f32> = Masker::new(Box::new(Silence::new()), 0, SAMPLE_RATE, CHANNELS);

    let batch_len = ms_to_stream_pos(BATCH_MS, SAMPLE_RATE, CHANNELS) as usize;
    let mut pending: Vec<(u64, TimedWord)> = WORDS
        .iter()
        .map(|&(w, s, e)| (e + recognition_latency_ms, TimedWord::new(w, s, e)))
        .collect();

    let mut output = Vec::new();
    let mut timings = Vec::new();
    let mut now_ms = 0;
    while now_ms < STREAM_MS {
        // 1–2. Новая порция звука — в задержку
        delay.push(&vec![ORIG; batch_len]);
        now_ms += BATCH_MS;

        // 3–4. Слова, которые «распознались» к этому моменту → интервалы → маска
        let (ready, rest): (Vec<_>, Vec<_>) =
            pending.into_iter().partition(|(at, _)| *at <= now_ms);
        pending = rest;
        let words: Vec<TimedWord> = ready.into_iter().map(|(_, w)| w).collect();
        for interval in engine.get_batch_ban_time(&words).unwrap_or_default() {
            timings.push(masker.add_interval(&interval));
        }

        // 5–6. Отлежавшийся звук → маска → выход
        output.extend(masker.process(&delay.pop_ready()));
    }
    // Завершение: хвост задержки тоже проходит через маску
    output.extend(masker.process(&delay.finish()));

    Run { output, timings }
}

/// Значение выхода в момент `ms` для канала `channel`
fn at(output: &[f32], ms: u64, channel: u16) -> f32 {
    output[(ms_to_stream_pos(ms, SAMPLE_RATE, CHANNELS) + channel as u64) as usize]
}

fn assert_masked(output: &[f32], from_ms: u64, to_ms: u64) {
    // Без краёв: на них фейд
    for ms in (from_ms + FADE_MS)..=(to_ms - FADE_MS) {
        for channel in 0..CHANNELS {
            let v = at(output, ms, channel);
            assert!(
                v.abs() < EPS,
                "{ms} мс, канал {channel}: ожидалась тишина, получено {v}"
            );
        }
    }
}

fn assert_original(output: &[f32], from_ms: u64, to_ms: u64) {
    for ms in from_ms..to_ms {
        for channel in 0..CHANNELS {
            let v = at(output, ms, channel);
            assert!(
                (v - ORIG).abs() < EPS,
                "{ms} мс, канал {channel}: ожидался звук, получено {v}"
            );
        }
    }
}

#[test]
fn whole_stream_comes_out() {
    let run = run(200);
    let total = ms_to_stream_pos(STREAM_MS, SAMPLE_RATE, CHANNELS) as usize;
    assert_eq!(
        run.output.len(),
        total,
        "ни один сэмпл не потерян и не добавлен"
    );
}

#[test]
fn banned_words_are_masked_when_recognition_is_fast_enough() {
    // Распознавание 200 мс < задержки 500 мс — оба слова успевают
    let run = run(200);

    assert_eq!(run.timings, vec![IntervalTiming::OnTime; 2]);
    assert_masked(&run.output, 1000, 1300); // «дурак»
    assert_masked(&run.output, 1800, 2100); // «Болван!» — регистр и знаки не мешают
}

#[test]
fn other_words_and_pauses_are_untouched() {
    let run = run(200);

    assert_original(&run.output, 0, 1000); // «привет» и тишина до «дурак»
    assert_original(&run.output, 1301, 1800); // «мир» между запрещёнными словами
    assert_original(&run.output, 2101, STREAM_MS);
}

#[test]
fn slow_recognition_lets_word_through() {
    // Распознавание 600 мс > задержки 500 мс: к моменту решения слово уже прозвучало
    let run = run(600);

    assert!(run.timings.iter().all(|t| t.is_late()), "{:?}", run.timings);
    assert_original(&run.output, 0, STREAM_MS);
}

#[test]
fn borderline_recognition_masks_only_the_rest_of_word() {
    // Распознавание 400 мс: слово «дурак» (300 мс) успевает выйти лишь на 200 мс,
    // остаток (с 1200 мс) ещё в задержке и глушится
    let run = run(400);

    assert!(
        run.timings
            .iter()
            .all(|&t| t == IntervalTiming::PartiallyLate),
        "{:?}",
        run.timings
    );
    assert_original(&run.output, 1000, 1190);
    assert_masked(&run.output, 1210, 1300);
}
