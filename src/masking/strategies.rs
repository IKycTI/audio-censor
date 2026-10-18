use dasp::{Sample, sample::Duplex};
use serde::Deserialize;
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum MaskingStrategy {
    Beep,
    Silence,
    MuteAttenuate,
}

pub trait Masking<T: Copy + Sample + Duplex<f32>> {
    fn apply(
        &self,
        original: &[T],
        sample_rate: u32,
        offset_interval: u64,
        channels: u16,
    ) -> Vec<T>;
}

#[derive(Default)]
pub struct Silence {}

impl Silence {
    pub fn new() -> Self {
        Silence {}
    }
}

impl<T: Copy + Sample + Duplex<f32>> Masking<T> for Silence {
    fn apply(
        &self,
        original: &[T],
        _sample_rate: u32,
        _offset_interval: u64,
        _channels: u16,
    ) -> Vec<T> {
        vec![T::EQUILIBRIUM; original.len()]
    }
}

pub struct Beep {
    frequency_hz: f32,
    amplitude: f32,
}

impl Beep {
    pub fn new(frequency_hz: f32, amplitude: f32) -> Self {
        Beep {
            frequency_hz,
            amplitude,
        }
    }
}

impl<T: Copy + Sample + Duplex<f32>> Masking<T> for Beep {
    fn apply(
        &self,
        original: &[T],
        sample_rate: u32,
        offset_interval: u64,
        channels: u16,
    ) -> Vec<T> {
        original
            .iter()
            .enumerate()
            .map(|(n, _)| {
                let val = self.amplitude
                    * (2f32
                        * PI
                        * self.frequency_hz
                        * ((offset_interval + n as u64) / channels as u64) as f32
                        / sample_rate as f32)
                        .sin();
                T::from_sample(val.clamp(-1f32, 1f32))
            })
            .collect()
    }
}

pub struct MuteAttenuate {
    factor: f32,
}

impl MuteAttenuate {
    pub fn new(attenuation_db: f32) -> Self {
        MuteAttenuate {
            factor: 10f32.powf(attenuation_db / 20f32),
        }
    }
}

impl<T: Copy + Sample + Duplex<f32>> Masking<T> for MuteAttenuate {
    fn apply(
        &self,
        original: &[T],
        _sample_rate: u32,
        _offset_interval: u64,
        _channels: u16,
    ) -> Vec<T> {
        original
            .iter()
            .map(|s| {
                let v = s.to_sample::<f32>();
                T::from_sample((v * self.factor).clamp(-1f32, 1f32))
            })
            .collect()
    }
}
