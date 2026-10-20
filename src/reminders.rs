//! Reminder timing only. This module does not connect to Google or fetch events.
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CalendarReminder {
    pub method: String,
    pub minutes: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventReminders {
    pub use_default: bool,
    #[serde(default)]
    pub overrides: Vec<CalendarReminder>,
}

/// Google uses the calendar's defaults or the event's explicit overrides.
/// An empty list means no reminders. Email reminders are delivered by Google.
/// The start is an absolute timestamp, already resolved in the event's time zone.
pub fn popup_times(start: i64, event: &EventReminders, defaults: &[CalendarReminder]) -> Vec<i64> {
    let reminders = if event.use_default {
        defaults
    } else {
        &event.overrides
    };
    let mut times: Vec<_> = reminders
        .iter()
        .filter(|r| r.method == "popup" && (0..=40_320).contains(&r.minutes))
        .filter_map(|r| start.checked_sub(r.minutes * 60))
        .collect();
    times.sort_unstable();
    times.dedup();
    times
}

#[cfg(test)]
mod tests {
    use super::*;
    fn popup(minutes: i64) -> CalendarReminder {
        CalendarReminder {
            method: "popup".into(),
            minutes,
        }
    }
    #[test]
    fn uses_google_defaults_without_inventing_a_local_offset() {
        let event = EventReminders {
            use_default: true,
            overrides: vec![popup(5)],
        };
        assert_eq!(
            popup_times(10_000, &event, &[popup(30), popup(0)]),
            vec![8_200, 10_000]
        );
        assert!(popup_times(10_000, &event, &[]).is_empty());
    }
    #[test]
    fn explicit_overrides_replace_defaults_and_empty_means_disabled() {
        let mut event = EventReminders {
            use_default: false,
            overrides: vec![
                popup(7),
                popup(7),
                CalendarReminder {
                    method: "email".into(),
                    minutes: 60,
                },
            ],
        };
        assert_eq!(popup_times(10_000, &event, &[popup(10)]), vec![9_580]);
        event.overrides.clear();
        assert!(popup_times(10_000, &event, &[popup(10)]).is_empty());
    }
    #[test]
    fn absolute_timestamps_and_invalid_values_do_not_shift_reminders() {
        let event = EventReminders {
            use_default: false,
            overrides: vec![popup(-1), popup(40_321), popup(1)],
        };
        assert_eq!(popup_times(1_792_343_400, &event, &[]), vec![1_792_343_340]);
        assert!(popup_times(i64::MIN, &event, &[]).is_empty());
    }
}
