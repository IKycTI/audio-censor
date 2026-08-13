use crate::ms_to_samples;

pub struct Chunker {
    chunk_size_sample: u64,
    step_samples: u64,
    interval_buffer: Vec<i16>,
    next_chunk_start_pos: u64,
}

pub struct Chunk {
    pub data: Vec<i16>,
    pub start_sample: u64,
}

impl Chunker {
    pub fn new(chunk_size_ms: u64, overlap_ms: u64, sample_rate: u32) -> Self {
        let chunk_size_sample = ms_to_samples(chunk_size_ms, sample_rate);
        let overlap_size_samples = ms_to_samples(overlap_ms, sample_rate);

        let step_samples = chunk_size_sample - overlap_size_samples;

        Chunker {
            chunk_size_sample,
            step_samples,
            interval_buffer: Vec::new(),
            next_chunk_start_pos: 0,
        }
    }

    pub fn push_samples(&mut self, sample: &[i16]) -> Vec<Chunk> {
        self.interval_buffer.extend_from_slice(sample);
        let mut chunk_vec = Vec::<Chunk>::new();

        while self.interval_buffer.len() >= self.chunk_size_sample as usize {
            let chunk_data: Vec<i16> =
                self.interval_buffer[0..self.chunk_size_sample as usize].to_vec();
            let chunk_start = self.next_chunk_start_pos;

            chunk_vec.push(Chunk {
                data: chunk_data,
                start_sample: chunk_start,
            });

            self.interval_buffer.drain(0..self.step_samples as usize);
            self.next_chunk_start_pos += self.step_samples;
        }
        chunk_vec
    }

    pub fn finish(mut self) -> Option<Chunk> {
        let len_buf = self.interval_buffer.len();
        println!("Len buffer: {}", len_buf);
        if len_buf == 0 {
            return None;
        }

        let padding_needed = self.chunk_size_sample as usize - len_buf;
        self.interval_buffer.extend(vec![0i16; padding_needed]);
        Some(Chunk {
            data: self.interval_buffer,
            start_sample: self.next_chunk_start_pos,
        })
    }
}
