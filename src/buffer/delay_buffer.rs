use std::collections::VecDeque;

use crate::ms_to_samples;

pub struct DelayBuffer<T> {
    samples: VecDeque<T>,
    next_pos: u64,
    oldest_pos: u64,
    delay: u64,
}

impl<T: Copy> DelayBuffer<T> {
    pub fn new(delay_ms: u64, sample_rate: u32) -> Self {
        let delay = ms_to_samples(delay_ms, sample_rate);
        DelayBuffer {
            samples: VecDeque::<T>::new(),
            next_pos: 0,
            oldest_pos: 0,
            delay,
        }
    }

    pub fn push(&mut self, samples: &[T]) {
        self.samples.extend(samples.iter().copied());
        self.next_pos += samples.len() as u64;
    }

    pub fn pop_ready(&mut self) -> Vec<(u64, T)> // (Позиция, значение)
    {
        let count_ready = self.samples.len().saturating_sub(self.delay as usize);

        if count_ready == 0 {
            return Vec::new();
        }

        let result: Vec<(u64, T)> = self
            .samples
            .drain(0..count_ready)
            .enumerate()
            .map(|(i, s)| (self.oldest_pos + i as u64, s))
            .collect();

        self.oldest_pos += count_ready as u64;

        result
    }

    pub fn finish(mut self) -> Vec<(u64, T)> {
        self.samples
            .drain(..)
            .enumerate()
            .map(|(i, s)| (self.oldest_pos + i as u64, s))
            .collect()
    }
}
