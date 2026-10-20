use chrono::{Datelike, Duration, NaiveDate};

pub const MONTHS: [&str; 12] = [
    "Enero",
    "Febrero",
    "Marzo",
    "Abril",
    "Mayo",
    "Junio",
    "Julio",
    "Agosto",
    "Septiembre",
    "Octubre",
    "Noviembre",
    "Diciembre",
];
pub const WEEKDAYS: [&str; 7] = [
    "lunes",
    "martes",
    "miércoles",
    "jueves",
    "viernes",
    "sábado",
    "domingo",
];

pub fn month_name(date: NaiveDate) -> &'static str {
    MONTHS[date.month0() as usize]
}

pub fn date_label(date: NaiveDate) -> String {
    format!(
        "{}, {} de {}",
        WEEKDAYS[date.weekday().num_days_from_monday() as usize],
        date.day(),
        month_name(date).to_lowercase()
    )
}

pub fn week_start(date: NaiveDate, monday: bool) -> NaiveDate {
    let offset = if monday {
        date.weekday().num_days_from_monday()
    } else {
        date.weekday().num_days_from_sunday()
    };
    date - Duration::days(offset.into())
}

pub fn month_days(date: NaiveDate, monday: bool) -> Vec<NaiveDate> {
    let first = date.with_day(1).expect("valid first day");
    let start = week_start(first, monday);
    let next = shift_month(first, 1);
    let last = next - Duration::days(1);
    let count = ((last - start).num_days() / 7 + 1) * 7;
    (0..count).map(|n| start + Duration::days(n)).collect()
}

pub fn shift_month(date: NaiveDate, delta: i32) -> NaiveDate {
    let index = date.year() * 12 + date.month0() as i32 + delta;
    let year = index.div_euclid(12);
    let month = index.rem_euclid(12) as u32 + 1;
    (1..=date.day())
        .rev()
        .find_map(|day| NaiveDate::from_ymd_opt(year, month, day))
        .unwrap_or(date)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn month_grid_has_real_dates_and_complete_weeks() {
        for year in [2024, 2025, 2026, 2100] {
            for month in 1..=12 {
                for monday in [true, false] {
                    let days = month_days(d(year, month, 15), monday);
                    assert_eq!(days.len() % 7, 0);
                    assert!((28..=42).contains(&days.len()));
                    assert_eq!(days[0], week_start(days[0], monday));
                    let in_month: Vec<_> = days.iter().filter(|x| x.month() == month).collect();
                    assert_eq!(in_month.first().unwrap().day(), 1);
                    assert_eq!(
                        in_month.last().unwrap().succ_opt().unwrap().month(),
                        month % 12 + 1
                    );
                    for pair in days.windows(2) {
                        assert_eq!(pair[1] - pair[0], Duration::days(1));
                    }
                }
            }
        }
    }

    #[test]
    fn navigation_clamps_month_end_and_crosses_year() {
        assert_eq!(shift_month(d(2024, 1, 31), 1), d(2024, 2, 29));
        assert_eq!(shift_month(d(2025, 1, 31), 1), d(2025, 2, 28));
        assert_eq!(shift_month(d(2026, 1, 5), -1), d(2025, 12, 5));
        assert_eq!(week_start(d(2026, 1, 1), true), d(2025, 12, 29));
    }
}
