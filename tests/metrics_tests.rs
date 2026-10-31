use audio_censor::metrics::Metrics;

#[test]
fn latency_is_null_before_first_word() {
    // Ноль выглядел бы как мгновенное распознавание
    let snap = Metrics::default().snapshot();
    assert_eq!(snap.recognition_latency_samples, 0);
    assert_eq!(snap.recognition_latency_avg_ms, None);
    assert_eq!(snap.recognition_latency_max_ms, None);
}

#[test]
fn latency_average_and_maximum() {
    let metrics = Metrics::default();
    for latency in [3100, 4300, 2900] {
        metrics.record_recognition_latency(latency);
    }

    let snap = metrics.snapshot();
    assert_eq!(snap.recognition_latency_samples, 3);
    assert_eq!(snap.recognition_latency_avg_ms, Some(3433));
    assert_eq!(snap.recognition_latency_max_ms, Some(4300));
}

#[test]
fn zero_latency_is_recorded() {
    let metrics = Metrics::default();
    metrics.record_recognition_latency(0);

    let snap = metrics.snapshot();
    assert_eq!(snap.recognition_latency_avg_ms, Some(0));
    assert_eq!(snap.recognition_latency_max_ms, Some(0));
}

#[test]
fn latency_is_serialized_as_null_when_absent() {
    let json = serde_json::to_string(&Metrics::default().snapshot()).unwrap();
    assert!(
        json.contains("\"recognition_latency_avg_ms\":null"),
        "{json}"
    );
}

#[test]
fn chunk_processing_is_null_before_first_chunk() {
    let snap = Metrics::default().snapshot();
    assert_eq!(snap.chunks_processed, 0);
    assert_eq!(snap.chunk_processing_avg_us, None);
    assert_eq!(snap.chunk_processing_max_us, None);
}

#[test]
fn chunk_processing_average_and_maximum() {
    let metrics = Metrics::default();
    for processing in [12_000, 30_500, 9_000] {
        metrics.record_chunk_processing(processing);
    }

    let snap = metrics.snapshot();
    assert_eq!(snap.chunks_processed, 3);
    assert_eq!(snap.chunk_processing_avg_us, Some(17_166));
    assert_eq!(snap.chunk_processing_max_us, Some(30_500));
}
