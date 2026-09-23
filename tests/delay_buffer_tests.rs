use audio_censor::buffer::DelayBuffer;

/// Сэмпл-рейт 1000 Гц → 1 мс = 10 сэмплов.
const SAMPLE_RATE: u32 = 1000;

// ── Базовое поведение ──────────────────────────────────────────────

#[test]
fn new_buffer_is_empty() {
    let mut buf: DelayBuffer<i16> = DelayBuffer::new(10, SAMPLE_RATE);
    assert!(buf.pop_ready().is_empty());
}

#[test]
fn push_then_pop_ready_returns_samples() {
    let mut buf = DelayBuffer::new(0, SAMPLE_RATE); // задержка 0 → сразу
    buf.push(&[100, 200, 300]);
    let ready = buf.pop_ready();
    assert_eq!(ready.len(), 3);
    assert_eq!(ready, vec![(0, 100), (1, 200), (2, 300)]);
}

#[test]
fn pop_ready_returns_empty_when_all_delayed() {
    let mut buf = DelayBuffer::new(100, SAMPLE_RATE); // 100 мс = 100 сэмплов задержки
    buf.push(&[1, 2, 3]);
    assert!(buf.pop_ready().is_empty());
}

#[test]
fn pop_ready_returns_partial_when_some_samples_ready() {
    // Задержка = 5 мс = 5 сэмплов. Подаём 8 — должны получить первые 3.
    let mut buf = DelayBuffer::new(5, SAMPLE_RATE);
    buf.push(&[10, 20, 30, 40, 50, 60, 70, 80]);
    let ready = buf.pop_ready();
    assert_eq!(ready.len(), 3);
    assert_eq!(ready, vec![(0, 10), (1, 20), (2, 30)]);
}

// ── Позиционирование ───────────────────────────────────────────────

#[test]
fn positions_are_monotonic_across_pop_ready_calls() {
    let mut buf = DelayBuffer::new(0, SAMPLE_RATE);
    buf.push(&[1, 2, 3]);
    buf.push(&[4, 5]);

    // Задержка 0 → все 5 сэмплов готовы сразу
    let all = buf.pop_ready();
    assert_eq!(all.len(), 5);
    assert_eq!(all, vec![(0, 1), (1, 2), (2, 3), (3, 4), (4, 5)]);
}

#[test]
fn positions_are_correct_with_nonzero_delay() {
    let mut buf = DelayBuffer::new(2, SAMPLE_RATE); // 2 сэмпла задержки
    buf.push(&[10, 20, 30, 40, 50]);

    // 5 сэмплов в буфере, задержка 2 → ready = 5 - 2 = 3
    let ready = buf.pop_ready();
    assert_eq!(ready.len(), 3);
    assert_eq!(ready[0], (0, 10));
    assert_eq!(ready[2], (2, 30));
}

// ── Множественные push/pop ─────────────────────────────────────────

#[test]
fn multiple_push_pop_cycles_work_correctly() {
    let mut buf = DelayBuffer::new(0, SAMPLE_RATE);

    buf.push(&[1, 2]);
    assert_eq!(buf.pop_ready(), vec![(0, 1), (1, 2)]);

    buf.push(&[3, 4, 5]);
    let ready = buf.pop_ready();
    assert_eq!(ready.len(), 3);
    assert_eq!(ready[0].1, 3);
    assert_eq!(ready[1].1, 4);
    assert_eq!(ready[2].1, 5);
    // Позиции продолжаются с 2
    assert_eq!(ready[0].0, 2);
    assert_eq!(ready[2].0, 4);
}

#[test]
fn pop_ready_drains_only_ready_samples_leaving_rest_in_buffer() {
    let mut buf = DelayBuffer::new(3, SAMPLE_RATE); // 3 сэмпла задержки
    buf.push(&[1, 2, 3, 4, 5, 6]);

    // 6 сэмплов в буфере, задержка 3 → ready = 6 - 3 = 3
    let ready = buf.pop_ready();
    assert_eq!(ready.len(), 3);
    assert_eq!(ready, vec![(0, 1), (1, 2), (2, 3)]);

    // Остались [4, 5, 6] в буфере, но задержка всё ещё 3 → ready = 3-3 = 0
    assert!(buf.pop_ready().is_empty());

    // Добавляем ещё → теперь 7 сэмплов в буфере (3 старых + 4 новых), ready = 7-3 = 4
    buf.push(&[7, 8, 9, 10]);
    let ready2 = buf.pop_ready();
    assert_eq!(ready2.len(), 4);
    assert_eq!(ready2[0], (3, 4));
    assert_eq!(ready2[3], (6, 7));
}

// ── finish() ───────────────────────────────────────────────────────

#[test]
fn finish_drains_all_remaining_samples() {
    let mut buf = DelayBuffer::new(5, SAMPLE_RATE);
    buf.push(&[100, 200, 300]);
    // Задержка 5 > 3 сэмпла → pop_ready ничего не отдаст
    assert!(buf.pop_ready().is_empty());

    let drained = buf.finish();
    assert_eq!(drained.len(), 3);
    assert_eq!(drained, vec![(0, 100), (1, 200), (2, 300)]);
}

#[test]
fn finish_on_empty_buffer_returns_empty_vec() {
    let buf: DelayBuffer<i16> = DelayBuffer::new(10, SAMPLE_RATE);
    assert!(buf.finish().is_empty());
}

#[test]
fn finish_after_partial_pop() {
    let mut buf = DelayBuffer::new(0, SAMPLE_RATE);
    buf.push(&[1, 2, 3, 4, 5]);

    // Задержка 0 → все сэмплы готовы сразу
    let first = buf.pop_ready();
    assert_eq!(first.len(), 5);
    assert_eq!(first, vec![(0, 1), (1, 2), (2, 3), (3, 4), (4, 5)]);

    // Буфер пуст
    let remaining = buf.finish();
    assert!(remaining.is_empty());
}

// ── Разные типы сэмплов ────────────────────────────────────────────

#[test]
fn delay_buffer_works_with_f32() {
    let mut buf = DelayBuffer::new(0, SAMPLE_RATE);
    buf.push(&[0.5, -0.5, 1.0]);
    let ready = buf.pop_ready();
    assert_eq!(ready.len(), 3);
    assert!((ready[0].1 - 0.5_f32).abs() < f32::EPSILON);
    assert!((ready[1].1 - (-0.5_f32)).abs() < f32::EPSILON);
    assert!((ready[2].1 - 1.0_f32).abs() < f32::EPSILON);
}

#[test]
fn delay_buffer_works_with_u8() {
    let mut buf = DelayBuffer::new(0, SAMPLE_RATE);
    buf.push(&[0u8, 128, 255]);
    let ready = buf.pop_ready();
    assert_eq!(ready, vec![(0, 0u8), (1, 128), (2, 255)]);
}

// ── Граничные случаи ───────────────────────────────────────────────

#[test]
fn zero_delay_means_immediate_read() {
    let mut buf = DelayBuffer::new(0, SAMPLE_RATE);
    buf.push(&[42]);
    assert_eq!(buf.pop_ready(), vec![(0, 42)]);
}

#[test]
fn large_delay_blocks_all_samples() {
    let mut buf = DelayBuffer::new(60_000, SAMPLE_RATE); // 60 секунд задержки
    buf.push(&[1, 2, 3]);
    assert!(buf.pop_ready().is_empty());
    let drained = buf.finish();
    assert_eq!(drained.len(), 3);
}

#[test]
fn empty_push_does_not_crash() {
    let mut buf = DelayBuffer::new(0, SAMPLE_RATE);
    buf.push(&[] as &[i16]);
    assert!(buf.pop_ready().is_empty());
}

#[test]
fn single_sample_push_pop_finish_chain() {
    let mut buf = DelayBuffer::new(0, SAMPLE_RATE);
    buf.push(&[999]);
    assert_eq!(buf.pop_ready(), vec![(0, 999)]);
    assert!(buf.finish().is_empty());
}

#[test]
fn saturating_sub_prevents_underflow_in_pop_ready() {
    // Если delay больше количества сэмплов, saturating_sub должен
    // вернуть 0, а не вызвать panic при переполнении.
    let mut buf = DelayBuffer::new(1000, SAMPLE_RATE); // 1000 сэмплов задержки
    buf.push(&[1]);
    // Не должно паниковать
    assert!(buf.pop_ready().is_empty());
}

#[test]
fn positions_continue_correctly_after_finish() {
    let mut buf = DelayBuffer::new(0, SAMPLE_RATE);
    buf.push(&[1, 2, 3]);
    let first = buf.pop_ready();
    assert_eq!(first, vec![(0, 1), (1, 2), (2, 3)]);

    buf.push(&[4, 5]);
    let second = buf.finish();
    assert_eq!(second, vec![(3, 4), (4, 5)]);
}
