use tracing::info;

use crate::ms_to_frames;

/// Нарезка моно-потока на чанки фиксированного размера, встык.
/// Перекрытия нет: Vosk — потоковый распознаватель и сам помнит контекст
/// между чанками, а повторно поданный звук сдвинул бы его таймстемпы
pub struct Chunker {
    chunk_size_sample: u64,
    interval_buffer: Vec<i16>,
}

pub struct Chunk {
    pub data: Vec<i16>,
}

impl Chunker {
    pub fn new(chunk_size_ms: u64, sample_rate: u32) -> Self {
        // Чанкер получает моно-поток, поэтому кадр = сэмпл
        let chunk_size_sample = ms_to_frames(chunk_size_ms, sample_rate);
        assert!(chunk_size_sample > 0, "Размер чанка должен быть больше 0");

        info!("Размер чанка: {}", chunk_size_sample);

        Chunker {
            chunk_size_sample,
            interval_buffer: Vec::new(),
        }
    }

    pub fn push_samples(&mut self, sample: &[i16]) -> Vec<Chunk> {
        self.interval_buffer.extend_from_slice(sample);
        let mut chunk_vec = Vec::<Chunk>::new();

        while self.interval_buffer.len() >= self.chunk_size_sample as usize {
            let chunk_data: Vec<i16> = self
                .interval_buffer
                .drain(0..self.chunk_size_sample as usize)
                .collect();

            chunk_vec.push(Chunk { data: chunk_data });
        }
        chunk_vec
    }

    pub fn finish(&mut self) -> Option<Chunk> {
        let len_buf = self.interval_buffer.len();
        if len_buf == 0 {
            return None;
        }

        let padding_needed = self.chunk_size_sample as usize - len_buf;
        self.interval_buffer.extend(vec![0i16; padding_needed]);
        Some(Chunk {
            data: std::mem::take(&mut self.interval_buffer),
        })
    }
}
