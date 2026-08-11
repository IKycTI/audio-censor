use cpal::{Sample, StreamConfig};

pub fn convert<T>(data: &[T], channels: usize) -> Vec<i16>
where
    T: Sample,
    f32: cpal::FromSample<T>,
{
    to_i16(to_mono(data, channels).as_slice())
}

fn to_mono<T>(data: &[T], channels: usize) -> Vec<f32>
where
    T: Sample,
    f32: cpal::FromSample<T>,
{
    data.chunks(channels)
        .map(|chunk| {
            let sum: f32 = chunk.iter().map(|&s| s.to_sample::<f32>()).sum();
            sum / channels as f32
        })
        .collect()
}

fn to_i16(data: &[f32]) -> Vec<i16> {
    data.iter()
        .map(|&sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16)
        .collect()
}
