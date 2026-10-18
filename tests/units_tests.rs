use audio_censor::{ms_to_frames, ms_to_stream_pos, stream_pos_to_ms, whole_frames};

// ── ms_to_frames ───────────────────────────────────────────────────

#[test]
fn ms_to_frames_is_exact_for_whole_values() {
    assert_eq!(ms_to_frames(1000, 44100), 44100);
    assert_eq!(ms_to_frames(3000, 44100), 132300);
    assert_eq!(ms_to_frames(20, 48000), 960);
    assert_eq!(ms_to_frames(0, 44100), 0);
}

#[test]
fn ms_to_frames_rounds_down() {
    // 1 мс при 44100 Гц = 44.1 кадра → 44
    assert_eq!(ms_to_frames(1, 44100), 44);
    assert_eq!(ms_to_frames(9, 44100), 396);
}

#[test]
fn ms_to_frames_does_not_lose_precision_on_long_streams() {
    // 24 часа при 48 кГц: прежняя формула на f32 давала 4147199744 — ошибка 256 кадров
    let day_ms = 24 * 3600 * 1000;
    assert_eq!(ms_to_frames(day_ms, 48000), 4_147_200_000);
}

// ── ms_to_stream_pos ───────────────────────────────────────────────

#[test]
fn ms_to_stream_pos_is_frames_times_channels() {
    for ms in [0, 1, 7, 999, 3000, 3200] {
        for channels in [1, 2, 6] {
            assert_eq!(
                ms_to_stream_pos(ms, 44100, channels),
                ms_to_frames(ms, 44100) * channels as u64
            );
        }
    }
}

// ── stream_pos_to_ms ───────────────────────────────────────────────

#[test]
fn stream_pos_to_ms_inverts_ms_to_stream_pos_for_whole_frames_per_ms() {
    // 48 и 16 кадров на мс — преобразование туда и обратно точное
    for ms in [0, 1, 7, 999, 3000, 86_400_000] {
        for (rate, channels) in [(48000, 1), (48000, 2), (16000, 6)] {
            let pos = ms_to_stream_pos(ms, rate, channels);
            assert_eq!(
                stream_pos_to_ms(pos, rate, channels),
                ms,
                "{ms} мс, {rate} Гц"
            );
        }
    }
}

#[test]
fn stream_pos_to_ms_round_trip_loses_at_most_one_ms_at_44100() {
    // 1 мс = 44.1 кадра: туда — вниз до 44 кадров, обратно 44 кадра = 0.998 мс → 0
    for ms in 0..2000 {
        let back = stream_pos_to_ms(ms_to_stream_pos(ms, 44100, 2), 44100, 2);
        assert!(ms - back <= 1, "{ms} мс → {back} мс");
    }
    assert_eq!(
        stream_pos_to_ms(ms_to_stream_pos(3000, 44100, 2), 44100, 2),
        3000
    );
}

#[test]
fn stream_pos_to_ms_rounds_down() {
    // 264601 — правый канал кадра 132300 = ровно 3000 мс; 264599 — ещё 2999 мс
    assert_eq!(stream_pos_to_ms(264601, 44100, 2), 3000);
    assert_eq!(stream_pos_to_ms(264599, 44100, 2), 2999);
}

// ── whole_frames ───────────────────────────────────────────────────

#[test]
fn whole_frames_cuts_partial_frame() {
    assert_eq!(whole_frames(7, 2), 6);
    assert_eq!(whole_frames(8, 2), 8);
    assert_eq!(whole_frames(11, 6), 6);
    assert_eq!(whole_frames(1, 2), 0);
    assert_eq!(whole_frames(0, 2), 0);
}

#[test]
fn whole_frames_keeps_mono_as_is() {
    for len in 0..20 {
        assert_eq!(whole_frames(len, 1), len);
    }
}
