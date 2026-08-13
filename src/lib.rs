pub mod buffer;
pub mod capture;
pub mod config;
pub mod decision;
pub mod framing;
pub mod logging;
pub mod masking;
pub mod playback;
pub mod recognition;

pub const MS_PER_SEC: f32 = 1000.0;

/// Convert a duration in milliseconds to a sample count at the given sample rate.
pub fn ms_to_samples(ms: u64, sample_rate: u32) -> u64 {
    (ms as f32 * sample_rate as f32 / MS_PER_SEC).round() as u64
}
