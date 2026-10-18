use std::marker::PhantomData;

use dasp::{Sample, sample::Duplex};

use crate::{decision::IntervalBanTime, masking::strategies::*, ms_to_stream_pos};

const DEFAULT_FADE_MS: u64 = 5;

/// Успел ли интервал зарегистрироваться до того, как слово ушло на выход
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalTiming {
    /// Слово ещё целиком в задержке — будет заглушено полностью
    OnTime,
    /// Начало слова уже прозвучало, заглушена только оставшаяся часть
    PartiallyLate,
    /// Слово прозвучало целиком, маскировать нечего
    Late,
}

impl IntervalTiming {
    pub fn is_late(self) -> bool {
        self != IntervalTiming::OnTime
    }
}
pub struct Masker<T>
where
    T: Copy + Sample + Duplex<f32>,
{
    strategy: Box<dyn Masking<T>>,
    padding_sample: u64,
    fade_sample: u64,
    sample_rate: u32,
    channels: u16,
    // Позиция, следующая за последним обработанным сэмплом
    next_pos: u64,
    active_intervals: Vec<ActiveInterval>,
    _marker: PhantomData<T>,
}

struct ActiveInterval {
    start: u64,
    end: u64,
    // Откуда отсчитывать фейд-ин: совпадает со start, если интервал пришёл вовремя,
    // иначе — первый кадр, который ещё не ушёл на выход
    fade_start: u64,
    // Начало отсчёта фазы стратегии: не меняется при слиянии, если маска уже звучит,
    // иначе бип на оставшейся части скачет по фазе
    phase_start: u64,
}

impl ActiveInterval {
    pub fn new(start: u64, end: u64, fade_start: u64) -> Self {
        ActiveInterval {
            start,
            end,
            fade_start,
            phase_start: start,
        }
    }
}

impl<T> Masker<T>
where
    T: Copy + Sample + Duplex<f32>,
{
    pub fn new(
        strategy: Box<dyn Masking<T>>,
        padding_ms: u64,
        sample_rate: u32,
        channels: u16,
    ) -> Self {
        let padding_sample = ms_to_stream_pos(padding_ms, sample_rate, channels);
        let fade_sample = ms_to_stream_pos(DEFAULT_FADE_MS, sample_rate, channels);

        Masker::<T> {
            strategy,
            padding_sample,
            fade_sample,
            sample_rate,
            channels,
            next_pos: 0,
            active_intervals: Vec::new(),
            _marker: PhantomData::default(),
        }
    }

    pub fn add_interval(&mut self, interval: &IntervalBanTime) -> IntervalTiming {
        let word_start = ms_to_stream_pos(interval.start_time, self.sample_rate, self.channels);
        let word_end = ms_to_stream_pos(interval.end_time, self.sample_rate, self.channels)
            + self.channels as u64
            - 1;

        // Опоздание считается по самому слову, без паддинга: паддинг — запас,
        // его потеря не означает, что слово было слышно
        let timing = if word_end < self.next_pos {
            IntervalTiming::Late
        } else if word_start < self.next_pos {
            IntervalTiming::PartiallyLate
        } else {
            IntervalTiming::OnTime
        };

        let start = word_start.saturating_sub(self.padding_sample);
        let end = word_end + self.padding_sample;

        // Если начало интервала уже ушло на выход, фейд-ин начинается с текущего кадра
        let channels = self.channels as u64;
        let fade_start = start.max(self.next_pos - self.next_pos % channels);
        let mut merged = ActiveInterval::new(start, end, fade_start);
        let next_pos = self.next_pos;
        let mut playing_phase = None;

        // Слить пересекающиеся и соседние интервалы в один, чтобы start и end
        // всегда были настоящими краями маски
        self.active_intervals.retain(|iv| {
            let touches = iv.start <= merged.end + 1 && merged.start <= iv.end + 1;
            if touches {
                merged.start = merged.start.min(iv.start);
                merged.end = merged.end.max(iv.end);
                merged.fade_start = merged.fade_start.min(iv.fade_start);
                // Уже звучащий интервал не больше одного: два таких покрывали бы
                // последнюю выданную позицию и были бы слиты раньше
                if iv.start < next_pos {
                    playing_phase = Some(iv.phase_start);
                }
            }
            !touches
        });

        // Ещё ничего не прозвучало — фаза от нового начала, иначе продолжается
        merged.phase_start = playing_phase.unwrap_or(merged.start);
        self.active_intervals.push(merged);

        timing
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

                    // Смещение от начала фазы по позиции в потоке, а не по счётчику
                    // обработанных сэмплов: phase_start выровнен по кадру, поэтому номер кадра
                    // и номер канала в стратегии получаются верными, даже если интервал
                    // пришёл с опозданием и сегмент начинается с середины кадра
                    let offset_interval = segment[0].0 - self.active_intervals[idx].phase_start;

                    let masked = self.strategy.apply(
                        &original,
                        self.sample_rate,
                        offset_interval,
                        self.channels,
                    );

                    output.extend(masked);
                }
            }
        }

        self.apply_fades(&mut output, input);
        self.cleanup_finished_intervals(input);

        if let Some(&(last_pos, _)) = input.last() {
            self.next_pos = last_pos + 1;
        }

        output
    }

    fn find_interval(&self, pos: u64) -> Option<usize> {
        self.active_intervals
            .iter()
            .position(|iv| pos >= iv.start && pos <= iv.end)
    }

    /// Фейды на краях маски. Доля маски в смеси считается от расстояния
    /// до краёв интервала в кадрах, а не от индекса в батче, поэтому фейд,
    /// разрезанный между батчами, продолжается без скачка
    fn apply_fades(&self, output: &mut [T], input: &[(u64, T)]) {
        let channels = self.channels as u64;
        let fade_frames = self.fade_sample / channels;
        if fade_frames == 0 {
            return;
        }

        for (i, &(pos, orig)) in input.iter().enumerate() {
            let Some(idx) = self.find_interval(pos) else {
                continue;
            };
            let iv = &self.active_intervals[idx];

            // Оба канала кадра получают одинаковый коэффициент
            let frames_from_start = pos.saturating_sub(iv.fade_start) / channels;
            let frames_to_end = (iv.end - pos) / channels;
            if frames_from_start >= fade_frames && frames_to_end >= fade_frames {
                continue;
            }

            let t_in = frames_from_start.min(fade_frames) as f32 / fade_frames as f32;
            let t_out = frames_to_end.min(fade_frames) as f32 / fade_frames as f32;
            let t = t_in.min(t_out);

            let orig_f32 = orig.to_sample::<f32>();
            let masked_f32 = output[i].to_sample::<f32>();
            output[i] = T::from_sample(orig_f32 * (1.0 - t) + masked_f32 * t);
        }
    }

    fn cleanup_finished_intervals(&mut self, input: &[(u64, T)]) {
        if let Some(&(max_pos, _)) = input.last() {
            self.active_intervals.retain(|iv| iv.end >= max_pos);
        }
    }
}
