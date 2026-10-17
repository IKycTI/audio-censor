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

/// Конвертация милисекунд в сэмплы
pub fn ms_to_samples(ms: u64, sample_rate: u32) -> u64 {
    (ms as f32 * sample_rate as f32 / MS_PER_SEC).round() as u64
}

pub fn ms_to_stream_pos(ms: u64, sample_rate: u32, channels: u16) -> u64 {
    ms * sample_rate as u64 / (MS_PER_SEC as u64) * channels as u64
}
