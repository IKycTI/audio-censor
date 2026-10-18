use std::time::{SystemTime, UNIX_EPOCH};

/// Сеанс работы демона: время запуска — общее имя файлов лога и метрик,
/// чтобы по имени было видно, какие метрики к какому логу относятся
#[derive(Debug, Clone)]
pub struct Session {
    /// Для имён файлов: `2026-10-18_13-37-54` (UTC, без `:`)
    pub id: String,
    /// Для содержимого: `2026-10-18T13:37:54Z`
    pub started_at: String,
}

impl Session {
    pub fn start() -> Self {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self::at(secs)
    }

    /// Сеанс, начатый в момент `secs` секунд от эпохи Unix
    pub fn at(secs: u64) -> Self {
        let t = UtcTime::from_unix(secs);
        Session {
            id: format!(
                "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
                t.year, t.month, t.day, t.hour, t.minute, t.second
            ),
            started_at: format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                t.year, t.month, t.day, t.hour, t.minute, t.second
            ),
        }
    }
}

/// Календарное время UTC. В стандартной библиотеке форматирования дат нет,
/// а для UTC (в отличие от местного времени) часовой пояс не нужен — хватает арифметики
struct UtcTime {
    year: i64,
    month: u32,
    day: u32,
    hour: u64,
    minute: u64,
    second: u64,
}

impl UtcTime {
    fn from_unix(secs: u64) -> Self {
        let (year, month, day) = civil_from_days((secs / 86_400) as i64);
        let time_of_day = secs % 86_400;
        UtcTime {
            year,
            month,
            day,
            hour: time_of_day / 3600,
            minute: time_of_day % 3600 / 60,
            second: time_of_day % 60,
        }
    }
}

/// Дни от 1970-01-01 → (год, месяц, день) по григорианскому календарю.
/// Алгоритм Говарда Хиннанта (`civil_from_days`)
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // день эры [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // год эры [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // день года, с 1 марта [0, 365]
    let mp = (5 * doy + 2) / 153; // месяц, с марта [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
