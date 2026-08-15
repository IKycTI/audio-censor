use std::marker::PhantomData;

use dasp::{sample::Duplex, Sample};

use crate::{decision::IntervalBanTime, masking::strategies::*, ms_to_samples};

const DEFAULT_FADE_MS: u64 = 5;
pub struct Masker<T>
where
    T: Default + Copy + Sample + Duplex<f32>,
{
    strategy: Box<dyn Masking<T>>,
    padding_sample: u64,
    fade_sample: u64,
    sample_rate: u32,
    active_intervals: Vec<ActiveInterval>,
    _marker: PhantomData<T>,
}

struct ActiveInterval {
    start: u64,
    end: u64,
    samples_masked: u64,
}

impl ActiveInterval {
    pub fn new(start: u64, end: u64, samples_masked: u64) -> Self {
        ActiveInterval {
            start,
            end,
            samples_masked,
        }
    }
}

impl<T> Masker<T>
where
    T: Default + Copy + Sample + Duplex<f32>,
{
    pub fn new(strategy: Box<dyn Masking<T>>, padding_ms: u64, sample_rate: u32) -> Self {
        let padding_sample = ms_to_samples(padding_ms, sample_rate);
        let fade_sample = ms_to_samples(DEFAULT_FADE_MS, sample_rate);

        Masker::<T> {
            strategy,
            padding_sample,
            fade_sample,
            sample_rate,
            active_intervals: Vec::new(),
            _marker: PhantomData::default(),
        }
    }

    pub fn add_interval(&mut self, interval: &IntervalBanTime) {
        let start = interval.start_time.saturating_sub(self.padding_sample);
        let end = interval.end_time + self.padding_sample;

        self.active_intervals
            .push(ActiveInterval::new(start, end, 0));
    }

    pub fn process(&mut self, input: &[(u64, T)]) -> Vec<T> {
        let mut output = Vec::with_capacity(input.len());
        let mut i = 0;

        while i < input.len() {
            let (pos, _) = input[i];
            let interval_idx = self.find_interval(pos);

            match interval_idx {
                None => {
                    // Собрать подряд идущие сэмплы БЕЗ маски в один сегмент
                    let start = i;
                    while i < input.len() && self.find_interval(input[i].0).is_none() {
                        i += 1;
                    }
                    let segment = &input[start..i];
                    output.extend(segment.iter().map(|(_, s)| *s));
                }
                Some(idx) => {
                    // Собрать подряд идущие сэмплы ПОД ОДНИМ И ТЕМ ЖЕ интервалом в сегмент
                    let start = i;
                    while i < input.len() && self.find_interval(input[i].0) == Some(idx) {
                        i += 1;
                    }
                    let segment = &input[start..i];
                    let original: Vec<T> = segment.iter().map(|(_, s)| *s).collect();

                    let masked = self.strategy.apply(
                        &original,
                        self.sample_rate,
                        self.active_intervals[idx].samples_masked,
                    );

                    self.active_intervals[idx].samples_masked += original.len() as u64;

                    output.extend(masked);
                }
            }
        }

        self.apply_fades(&mut output, input);
        self.cleanup_finished_intervals(input);

        output
    }

    fn find_interval(&self, pos: u64) -> Option<usize> {
        self.active_intervals
            .iter()
            .position(|iv| pos >= iv.start && pos <= iv.end)
    }
    fn apply_fades(&self, output: &mut [T], input: &[(u64, T)]) {
        let fade_samples = self.fade_sample as usize;
        let mut i = 0;
        while i < input.len() {
            let pos = input[i].0;
            let currently_masked = self.find_interval(pos).is_some();

            // переход "не маска → маска": затухание оригинала / нарастание маски
            if currently_masked && (i == 0 || self.find_interval(input[i - 1].0).is_none()) {
                for k in 0..fade_samples.min(output.len() - i) {
                    let t = k as f32 / fade_samples as f32;
                    let orig_f32 = input[i + k].1.to_sample::<f32>();
                    let masked_f32 = output[i + k].to_sample::<f32>();
                    let blended = orig_f32 * (1.0 - t) + masked_f32 * t;
                    output[i + k] = T::from_sample(blended);
                }
            }

            // переход "маска → не маска": обратное смешение
            if currently_masked
                && (i + 1 == input.len() || self.find_interval(input[i + 1].0).is_none())
            {
                for k in 0..fade_samples.min(i + 1) {
                    let idx = i - k;
                    let t = k as f32 / fade_samples as f32;
                    let orig_f32 = input[idx].1.to_sample::<f32>();
                    let masked_f32 = output[idx].to_sample::<f32>();
                    let blended = orig_f32 * (1.0 - t) + masked_f32 * t;
                    output[idx] = T::from_sample(blended);
                }
            }

            i += 1;
        }
    }
    fn cleanup_finished_intervals(&mut self, input: &[(u64, T)]) {
        if let Some(&(max_pos, _)) = input.last() {
            self.active_intervals.retain(|iv| iv.end >= max_pos);
        }
    }
}
