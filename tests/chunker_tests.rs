use audio_censor::framing::{Chunk, Chunker};

/// Детерминированный "синтетический" аудио-поток
fn synth_samples(n: usize) -> Vec<i16> {
    (0..n).map(|i| ((i % 1000) as i16) - 500).collect()
}

fn uneven_split_sizes(total: usize, seed: u64) -> Vec<usize> {
    let mut sizes = Vec::new();
    let mut remaining = total;
    let mut state = seed;
    while remaining > 0 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let max_chunk = remaining.min(950);
        let size = 1 + (state as usize % max_chunk);
        sizes.push(size);
        remaining -= size;
    }
    sizes
}

/// Сравнивает два набора чанков по данным и стартовой позиции.
fn assert_chunks_eq(a: &[Chunk], b: &[Chunk]) {
    assert_eq!(a.len(), b.len(), "разное количество чанков");
    for (i, (ca, cb)) in a.iter().zip(b.iter()).enumerate() {
        assert_eq!(
            ca.start_sample, cb.start_sample,
            "start_sample различается в чанке #{i}"
        );
        assert_eq!(ca.data, cb.data, "данные различаются в чанке #{i}");
    }
}

fn expected_sizes(chunk_size_ms: u64, overlap_ms: u64, sample_rate: u32) -> (usize, usize, usize) {
    let chunk_size = (chunk_size_ms * sample_rate as u64 / 1000) as usize;
    let overlap = (overlap_ms * sample_rate as u64 / 1000) as usize;
    let step = chunk_size - overlap;
    (chunk_size, overlap, step)
}

#[test]
fn chunking_is_independent_of_push_fragmentation() {
    const TOTAL: usize = 10_000;
    const CHUNK_MS: u64 = 100;
    const OVERLAP_MS: u64 = 20;
    const SAMPLE_RATE: u32 = 1000;

    let samples = synth_samples(TOTAL);

    // Прогон 1: весь поток одним большим куском.
    let mut chunker_whole = Chunker::new(CHUNK_MS, OVERLAP_MS, SAMPLE_RATE);
    let mut chunks_whole = chunker_whole.push_samples(&samples);
    if let Some(tail) = chunker_whole.finish() {
        chunks_whole.push(tail);
    }

    // Прогон 2: тот же поток, поданный порциями случайного неровного размера.
    let splits = uneven_split_sizes(TOTAL, /*seed*/ 0x1234_5678_9abc_def0);
    assert_eq!(
        splits.iter().sum::<usize>(),
        TOTAL,
        "сплиты должны в сумме давать TOTAL"
    );

    let mut chunker_split = Chunker::new(CHUNK_MS, OVERLAP_MS, SAMPLE_RATE);
    let mut chunks_split = Vec::new();
    let mut offset = 0usize;
    for size in splits {
        let piece = &samples[offset..offset + size];
        chunks_split.extend(chunker_split.push_samples(piece));
        offset += size;
    }
    assert_eq!(offset, TOTAL);
    if let Some(tail) = chunker_split.finish() {
        chunks_split.push(tail);
    }

    assert_chunks_eq(&chunks_whole, &chunks_split);
    // На всякий случай убедимся, что чанки вообще были сгенерированы —
    // иначе сравнение выше тривиально проходит на пустых Vec'ах.
    assert!(!chunks_whole.is_empty());
}

#[test]
fn start_sample_grows_strictly_by_step() {
    const CHUNK_MS: u64 = 100;
    const OVERLAP_MS: u64 = 25;
    const SAMPLE_RATE: u32 = 1000;
    const TOTAL: usize = 5_000;

    let (_, _, step) = expected_sizes(CHUNK_MS, OVERLAP_MS, SAMPLE_RATE);

    let mut chunker = Chunker::new(CHUNK_MS, OVERLAP_MS, SAMPLE_RATE);
    let samples = synth_samples(TOTAL);
    let chunks = chunker.push_samples(&samples);

    assert!(
        chunks.len() >= 2,
        "нужно хотя бы 2 чанка, чтобы проверить рост start_sample"
    );

    for pair in chunks.windows(2) {
        let diff = pair[1].start_sample - pair[0].start_sample;
        assert_eq!(
            diff, step as u64,
            "start_sample должен расти строго на step_samples ({step})"
        );
    }
}

#[test]
fn overlap_between_consecutive_chunks_matches_byte_for_byte() {
    const CHUNK_MS: u64 = 200;
    const OVERLAP_MS: u64 = 50;
    const SAMPLE_RATE: u32 = 1000;
    const TOTAL: usize = 8_000;

    let (chunk_size, overlap, _) = expected_sizes(CHUNK_MS, OVERLAP_MS, SAMPLE_RATE);
    assert!(overlap > 0, "тест рассчитан на ненулевой overlap");

    let mut chunker = Chunker::new(CHUNK_MS, OVERLAP_MS, SAMPLE_RATE);
    let samples = synth_samples(TOTAL);
    let chunks = chunker.push_samples(&samples);

    assert!(
        chunks.len() >= 2,
        "нужно хотя бы 2 чанка для проверки перекрытия"
    );

    for pair in chunks.windows(2) {
        let tail_of_prev = &pair[0].data[chunk_size - overlap..];
        let head_of_next = &pair[1].data[..overlap];
        assert_eq!(
            tail_of_prev, head_of_next,
            "последние {overlap} сэмплов чанка N должны совпадать с первыми {overlap} сэмплами чанка N+1"
        );
    }
}

#[test]
fn finish_pads_incomplete_tail_with_zeros() {
    const CHUNK_MS: u64 = 100;
    const OVERLAP_MS: u64 = 20;
    const SAMPLE_RATE: u32 = 1000;

    let (chunk_size, _, step) = expected_sizes(CHUNK_MS, OVERLAP_MS, SAMPLE_RATE);

    // Специально берём длину, не кратную chunk_size/step, чтобы после
    // всех полных чанков в буфере остался "хвост".
    const TOTAL: usize = 250;
    let samples = synth_samples(TOTAL);

    let mut chunker = Chunker::new(CHUNK_MS, OVERLAP_MS, SAMPLE_RATE);
    let full_chunks = chunker.push_samples(&samples);

    let consumed = full_chunks.len() * step;
    let remainder_len = TOTAL - consumed;
    assert!(
        remainder_len > 0 && remainder_len < chunk_size,
        "тест ожидает непустой неполный хвост, получили remainder_len={remainder_len}"
    );

    let tail = chunker
        .finish()
        .expect("хвост не должен быть None, если в буфере остались данные");

    // Финальный чанк должен быть полной длины chunk_size, а не обрезан/пропущен.
    assert_eq!(tail.data.len(), chunk_size);

    // Первые remainder_len сэмплов — это реальные данные хвоста потока.
    let expected_real_part = &samples[TOTAL - remainder_len..TOTAL];
    assert_eq!(&tail.data[..remainder_len], expected_real_part);

    // Остаток должен быть дополнен нулями.
    assert!(
        tail.data[remainder_len..].iter().all(|&s| s == 0),
        "хвост чанка после реальных данных должен быть заполнен нулями"
    );
}

#[test]
fn finish_returns_none_when_buffer_is_empty() {
    const CHUNK_MS: u64 = 100;
    const OVERLAP_MS: u64 = 0;
    const SAMPLE_RATE: u32 = 1000;

    let (chunk_size, overlap, step) = expected_sizes(CHUNK_MS, OVERLAP_MS, SAMPLE_RATE);
    assert_eq!(overlap, 0);
    assert_eq!(step, chunk_size);

    let mut chunker = Chunker::new(CHUNK_MS, OVERLAP_MS, SAMPLE_RATE);
    // Подаём ровно 3 полных чанка без остатка — буфер должен опустеть полностью.
    let samples = synth_samples(chunk_size * 3);
    let chunks = chunker.push_samples(&samples);
    assert_eq!(chunks.len(), 3);

    assert!(
        chunker.finish().is_none(),
        "finish() должен вернуть None, если во внутреннем буфере не осталось данных"
    );
}
