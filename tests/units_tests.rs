use audio_censor::{ms_to_frames, ms_to_stream_pos, whole_frames};

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
