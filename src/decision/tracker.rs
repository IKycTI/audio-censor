use crate::decision::IntervalBanTime;

/// Интервал, найденный распознаванием, с пометкой, впервые ли он найден
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedInterval {
    pub interval: IntervalBanTime,
    /// true — слово найдено впервые (по нему считаются метрики),
    /// false — уточнение уже найденного: интервал шире прежнего, маска его поглотит
    pub is_new: bool,
}

/// Итог фразы: что отправить в маску и сколько частичных находок не подтвердилось
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PhraseOutcome {
    pub detected: Vec<DetectedInterval>,
    /// Слово было в частичной гипотезе, но в финальном результате его нет
    /// (Vosk передумал) — маска легла на слово, которого не было: ложное срабатывание
    pub unconfirmed: usize,
}

/// Учёт запрещённых слов текущей фразы между частичными гипотезами и финальным результатом.
///
/// Частичная гипотеза Vosk приходит на каждый чанк и содержит всю фразу целиком,
/// поэтому одно и то же слово встречается много раз, а его границы могут уточняться.
/// Трекер отдаёт каждое слово как новое один раз, дальше — только уточнения
#[derive(Default)]
pub struct PhraseTracker {
    // Найденные в частичных гипотезах интервалы фразы и подтверждены ли они финалом
    sent: Vec<(IntervalBanTime, bool)>,
}

impl PhraseTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Интервалы из очередной частичной гипотезы
    pub fn on_partial(&mut self, intervals: Vec<IntervalBanTime>) -> Vec<DetectedInterval> {
        let mut detected = Vec::new();
        for interval in intervals {
            match self.sent.iter_mut().find(|(s, _)| overlaps(s, &interval)) {
                Some((known, _)) => {
                    if let Some(wider) = widen(known, &interval) {
                        *known = wider.clone();
                        detected.push(DetectedInterval {
                            interval: wider,
                            is_new: false,
                        });
                    }
                }
                None => {
                    self.sent.push((interval.clone(), false));
                    detected.push(DetectedInterval {
                        interval,
                        is_new: true,
                    });
                }
            }
        }
        detected
    }

    /// Интервалы из финального результата фразы. Сбрасывает состояние для следующей фразы
    pub fn on_final(&mut self, intervals: Vec<IntervalBanTime>) -> PhraseOutcome {
        let mut detected = Vec::new();
        for interval in intervals {
            let mut found = false;
            for (known, confirmed) in self.sent.iter_mut().filter(|(s, _)| overlaps(s, &interval)) {
                found = true;
                *confirmed = true;
                if let Some(wider) = widen(known, &interval) {
                    *known = wider.clone();
                    detected.push(DetectedInterval {
                        interval: wider,
                        is_new: false,
                    });
                }
            }
            if !found {
                // Частичные гипотезы выключены или пропустили слово — оно новое
                detected.push(DetectedInterval {
                    interval,
                    is_new: true,
                });
            }
        }

        let unconfirmed = self.sent.iter().filter(|(_, confirmed)| !confirmed).count();
        self.sent.clear();
        PhraseOutcome {
            detected,
            unconfirmed,
        }
    }
}

fn overlaps(a: &IntervalBanTime, b: &IntervalBanTime) -> bool {
    a.start_time <= b.end_time && b.start_time <= a.end_time
}

/// Объединение, если `new` выходит за границы `known`, иначе None
fn widen(known: &IntervalBanTime, new: &IntervalBanTime) -> Option<IntervalBanTime> {
    if new.start_time < known.start_time || new.end_time > known.end_time {
        Some(IntervalBanTime::new(
            known.start_time.min(new.start_time),
            known.end_time.max(new.end_time),
        ))
    } else {
        None
    }
}
