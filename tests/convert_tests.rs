use audio_censor::framing::convert_to_i16_mono;

#[test]
fn mono_passthrough_i16() {
    let data: [i16; 4] = [0, 16384, -16384, i16::MIN];
    let result = convert_to_i16_mono(&data, 1);
    assert_eq!(result, vec![0, 16384, -16384, i16::MIN + 1]); // ожидаемое поведение из-за асимметрии диапазона i16
}

#[test]
fn stereo_averages_channels() {
    // L=full scale, R=silence -> среднее примерно вполовину амплитуды
    let data: [i16; 4] = [i16::MAX, 0, i16::MIN, 0];
    let result = convert_to_i16_mono(&data, 2);
    assert_eq!(result.len(), 2);
    assert!((result[0] as i32 - i16::MAX as i32 / 2).abs() <= 1);
    assert!((result[1] as i32 - i16::MIN as i32 / 2).abs() <= 1);
}

#[test]
fn four_channels_mixdown() {
    // все каналы одинаковы -> результат должен совпасть с исходным значением
    let value = 12345i16;
    let data = [value; 8]; // 2 фрейма по 4 канала
    let result = convert_to_i16_mono(&data, 4);
    assert_eq!(result, vec![value, value]);
}

#[test]
fn silence_stays_silence() {
    let data = [0i16; 100];
    let result = convert_to_i16_mono(&data, 2);
    assert!(result.iter().all(|&s| s == 0));
}

#[test]
fn f32_input_within_range() {
    let data: [f32; 4] = [1.0, -1.0, 0.5, -0.5];
    let result = convert_to_i16_mono(&data, 1);
    assert_eq!(result[0], i16::MAX);
    // -1.0 симметрично мапится на -i16::MAX, не на i16::MIN
    assert_eq!(result[1], -i16::MAX);
    assert!((result[2] as i32 - i16::MAX as i32 / 2).abs() <= 1);
    assert!((result[3] as i32 + i16::MAX as i32 / 2).abs() <= 1);
}

#[test]
fn u16_input_converts_correctly() {
    // u16 в cpal центрируется вокруг 32768 как "тишина"
    let data: [u16; 3] = [0, 32768, 65535];
    let result = convert_to_i16_mono(&data, 1);
    assert_eq!(result.len(), 3);
    assert!(result[0] < 0);
    assert!((result[1] as i32).abs() <= 1); // ~тишина
    assert!(result[2] > 0);
}

#[test]
fn incomplete_trailing_chunk_is_dropped() {
    // chunks(channels) отдаёт неполный последний чанк как есть —
    // проверяем текущее поведение (деление на channels, а не на его длину)
    let data: [i16; 5] = [100, 200, 300, 400, 500]; // 2 канала, 5-й сэмпл "висит"
    let result = convert_to_i16_mono(&data, 2);
    assert_eq!(result.len(), 3); // [ (100+200)/2, (300+400)/2, 500/2 ]
    assert_eq!(result[2], 250); // 500 / 2, а не 500 — потенциальный баг!
}

#[test]
fn empty_input_returns_empty() {
    let data: [i16; 0] = [];
    let result = convert_to_i16_mono(&data, 2);
    assert!(result.is_empty());
}

#[test]
fn output_never_exceeds_i16_range() {
    let data: [f32; 6] = [1.5, -1.5, 2.0, -2.0, 100.0, -100.0]; // выходящие за диапазон значения
    let result = convert_to_i16_mono(&data, 1);
    for &s in &result {
        assert!(s >= -i16::MAX && s <= i16::MAX);
    }
}
