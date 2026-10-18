pub mod buffer;
pub mod capture;
pub mod config;
pub mod control;
pub mod decision;
pub mod framing;
pub mod logging;
pub mod masking;
pub mod metrics;
pub mod pipeline;
pub mod playback;
pub mod recognition;

pub const MS_PER_SEC: f32 = 1000.0;

/// Миллисекунды → кадры (по одному сэмплу на каждый канал), с округлением вниз.
/// Для моно-потока распознавания кадр и сэмпл совпадают
pub fn ms_to_frames(ms: u64, sample_rate: u32) -> u64 {
    ms * sample_rate as u64 / MS_PER_SEC as u64
}

/// Миллисекунды → позиция в потоке чередующихся сэмплов. Результат всегда кратен `channels`
pub fn ms_to_stream_pos(ms: u64, sample_rate: u32, channels: u16) -> u64 {
    ms_to_frames(ms, sample_rate) * channels as u64
}

/// Позиция в потоке чередующихся сэмплов → миллисекунды от начала потока, с округлением вниз.
/// Обратна `ms_to_stream_pos` с точностью до кадра
pub fn stream_pos_to_ms(pos: u64, sample_rate: u32, channels: u16) -> u64 {
    pos / channels as u64 * MS_PER_SEC as u64 / sample_rate as u64
}

/// Длина, урезанная до целого числа кадров. Поток чередующихся сэмплов можно резать
/// только по границе кадра: потеря части кадра сдвигает каналы во всём остальном потоке
pub fn whole_frames(len: usize, channels: u16) -> usize {
    len - len % channels as usize
}
