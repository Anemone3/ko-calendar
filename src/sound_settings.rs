use serde::{Deserialize, Serialize};

pub const ADVANCE_SECONDS: i64 = 2 * 60;
pub const DEFAULT_FILE: &str = "ko-tify.mp3";
pub const ADVANCE_FILE: &str = "alarm.mp3";
pub const RENAMED_FILES: [(&str, &str); 2] = [
    ("kokonut.mp3", DEFAULT_FILE),
    ("anticipado.mp3", ADVANCE_FILE),
];

pub fn current_file(name: &str) -> &str {
    RENAMED_FILES
        .iter()
        .find_map(|(previous, current)| (*previous == name).then_some(*current))
        .unwrap_or(name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundKind {
    Notification,
    Advance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundSettings {
    pub enabled: bool,
    pub file: String,
    pub volume: u8,
    pub advance_enabled: bool,
    pub advance_file: String,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            file: DEFAULT_FILE.into(),
            volume: 80,
            advance_enabled: false,
            advance_file: ADVANCE_FILE.into(),
        }
    }
}

impl SoundSettings {
    pub fn enabled_for(&self, kind: SoundKind) -> bool {
        match kind {
            SoundKind::Notification => self.enabled,
            SoundKind::Advance => self.advance_enabled,
        }
    }
    pub fn file_for(&self, kind: SoundKind) -> &str {
        match kind {
            SoundKind::Notification => &self.file,
            SoundKind::Advance => &self.advance_file,
        }
    }
}

/// When Google already notifies at the early-alert time, emit one early alert, not two banners.
pub fn without_duplicate_notifications(
    normal: Vec<crate::model::Reminder>,
    advance: &[crate::model::Reminder],
) -> Vec<crate::model::Reminder> {
    normal
        .into_iter()
        .filter(|normal| {
            !advance.iter().any(|early| {
                normal.event_id.is_some()
                    && normal.event_id == early.event_id
                    && normal.due_at == early.due_at
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upgrade_keeps_notification_preferences_but_requires_opt_in_for_advance() {
        let settings: SoundSettings =
            serde_json::from_str(r#"{"enabled":false,"file":"personal.mp3","volume":30}"#).unwrap();
        assert!(!settings.enabled_for(SoundKind::Notification));
        assert!(!settings.enabled_for(SoundKind::Advance));
        assert_eq!(settings.file_for(SoundKind::Notification), "personal.mp3");
        assert_eq!(settings.file_for(SoundKind::Advance), "alarm.mp3");
        assert_eq!(settings.volume, 30);
        let settings = SoundSettings {
            advance_enabled: true,
            ..settings
        };
        assert!(settings.enabled_for(SoundKind::Advance));
        assert!(!settings.enabled_for(SoundKind::Notification));
    }
    #[test]
    fn coincident_alerts_are_coalesced_only_for_the_same_event_and_time() {
        let reminder = crate::model::Reminder {
            id: 1,
            title: "event".into(),
            body: String::new(),
            due_at: 700,
            important: false,
            state: "delivered".into(),
            event_id: Some("event-a".into()),
        };
        let mut other = reminder.clone();
        other.event_id = Some("event-b".into());
        let mut later = reminder.clone();
        later.due_at = 800;
        assert_eq!(
            without_duplicate_notifications(vec![reminder.clone(), other, later], &[reminder])
                .len(),
            2
        );
    }
}
