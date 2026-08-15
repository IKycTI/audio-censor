use dasp::{sample::Duplex, Sample};

pub fn convert_to_i16_mono<T>(data: &[T], channels: u16) -> Vec<i16>
where
    T: Sample + Duplex<f32>,
{
    to_i16(to_mono(data, channels).as_slice())
}

fn to_mono<T>(data: &[T], channels: u16) -> Vec<f32>
where
    T: Sample + Duplex<f32>,
{
    data.chunks(channels as usize)
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
