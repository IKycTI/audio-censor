use crate::{ms_to_frames, ms_to_stream_pos, whole_frames};

/// Целевая заполненность выходного буфера. Добавляется к общей задержке, но даёт запас
/// на неравномерность колбэка воспроизведения и на дрейф в обе стороны
pub const OUTPUT_TARGET_FILL_MS: u64 = 200;

// Сглаживание заполненности выходного буфера: колбэк воспроизведения забирает данные
// порциями, и мгновенное значение «пилит» — решать по нему нельзя
const FILL_EMA_ALPHA: f64 = 0.01;
// Не чаще одной коррекции за это время: до 50 кадров/с, т.е. ~0.1% —
// с запасом больше реального расхождения кварцев (обычно десятки ppm)
const CORRECTION_INTERVAL_MS: u64 = 20;

/// Компенсация дрейфа часов между устройствами ввода и вывода.
///
/// Вход и выход тактируются разными кварцами: если вход чуть быстрее, выходной буфер
/// растёт до переполнения, если медленнее — опустошается. Компенсатор держит
/// заполненность выходного буфера около цели, выбрасывая или дублируя по одному кадру.
/// Работает после `Masker`, поэтому позиции маскирования не затрагивает
pub struct DriftCompensator {
    channels: usize,
    target: usize,
    band: usize,
    correction_interval_frames: u64,
    frames_since_correction: u64,
    avg_fill: f64,
    primed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftCorrection {
    None,
    /// Буфер был пуст (старт или срыв воспроизведения): в начало добавлено столько кадров тишины
    Prefilled(usize),
    DroppedFrame,
    DuplicatedFrame,
}

impl DriftCompensator {
    /// `target_ms` — целевая заполненность выходного буфера, допуск — половина цели
    pub fn new(target_ms: u64, sample_rate: u32, channels: u16) -> Self {
        let target = ms_to_stream_pos(target_ms, sample_rate, channels) as usize;
        DriftCompensator {
            channels: channels as usize,
            target,
            band: whole_frames(target / 2, channels),
            correction_interval_frames: ms_to_frames(CORRECTION_INTERVAL_MS, sample_rate),
            frames_since_correction: 0,
            avg_fill: 0.0,
            primed: false,
        }
    }

    /// Подправить батч перед записью в выходной буфер.
    /// `fill` — сколько сэмплов уже лежит в выходном буфере, `silence` — тишина для типа T
    pub fn adjust<T: Copy>(
        &mut self,
        batch: &mut Vec<T>,
        fill: usize,
        silence: T,
    ) -> DriftCorrection {
        let channels = self.channels;
        if batch.len() < channels {
            return DriftCorrection::None;
        }

        // Пустой буфер — звук уже прервался; дотягивать по кадру слишком долго,
        // поэтому сразу дополняем тишиной до цели
        if !self.primed || fill == 0 {
            let missing = whole_frames(
                self.target.saturating_sub(fill + batch.len()),
                channels as u16,
            );
            batch.splice(0..0, std::iter::repeat_n(silence, missing));
            self.primed = true;
            self.avg_fill = (fill + batch.len()) as f64;
            self.frames_since_correction = 0;
            return DriftCorrection::Prefilled(missing / channels);
        }

        // Заполненность после записи этого батча
        let fill_after = (fill + batch.len()) as f64;
        self.avg_fill += (fill_after - self.avg_fill) * FILL_EMA_ALPHA;
        self.frames_since_correction += (batch.len() / channels) as u64;

        if self.frames_since_correction < self.correction_interval_frames {
            return DriftCorrection::None;
        }

        if self.avg_fill > (self.target + self.band) as f64 && batch.len() >= 2 * channels {
            batch.truncate(batch.len() - channels);
            self.frames_since_correction = 0;
            return DriftCorrection::DroppedFrame;
        }

        if self.avg_fill < self.target.saturating_sub(self.band) as f64 {
            let last_frame = batch[batch.len() - channels..].to_vec();
            batch.extend(last_frame);
            self.frames_since_correction = 0;
            return DriftCorrection::DuplicatedFrame;
        }

        DriftCorrection::None
    }
}
