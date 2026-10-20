use chrono::{Duration, Local, NaiveDate, NaiveTime, TimeZone};
use regex_automata::meta::Regex;
use serde::{Deserialize, Serialize};
use std::{fmt, sync::LazyLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    Meet,
    Zoom,
    Teams,
    Link,
    Place,
    Focus,
}

impl EventKind {
    pub const ALL: [Self; 6] = [
        Self::Focus,
        Self::Meet,
        Self::Zoom,
        Self::Teams,
        Self::Link,
        Self::Place,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Meet => "Google Meet",
            Self::Zoom => "Zoom",
            Self::Teams => "Microsoft Teams",
            Self::Link => "Enlace",
            Self::Place => "Presencial",
            Self::Focus => "Tiempo personal",
        }
    }
    pub fn action(self) -> &'static str {
        match self {
            Self::Meet => "Abrir Google Meet",
            Self::Zoom => "Abrir Zoom",
            Self::Teams => "Abrir Microsoft Teams",
            Self::Link => "Abrir enlace",
            Self::Place => "Ver ubicación",
            Self::Focus => "Ver detalles",
        }
    }
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    #[serde(default)]
    pub all_day: bool,
    pub id: String,
    pub title: String,
    pub date: NaiveDate,
    pub start: NaiveTime,
    pub duration_minutes: i64,
    pub kind: EventKind,
    pub calendar: String,
    pub description: String,
    pub destination: String,
    pub important: bool,
    pub demo: bool,
}

impl Event {
    pub fn display_kind(&self) -> EventKind {
        match web_domain(&self.destination).as_deref() {
            Some("meet.google.com") => EventKind::Meet,
            Some(host) if host == "zoom.us" || host.ends_with(".zoom.us") => EventKind::Zoom,
            Some("teams.microsoft.com" | "teams.live.com") => EventKind::Teams,
            Some(_) if self.kind == EventKind::Place => EventKind::Link,
            _ => self.kind,
        }
    }
    pub fn time_label(&self, use_24h: bool) -> String {
        if self.all_day {
            return "Todo el día".into();
        }
        self.start
            .format(if use_24h { "%H:%M" } else { "%I:%M %p" })
            .to_string()
    }
    pub fn end_label(&self, use_24h: bool) -> String {
        if self.all_day {
            return String::new();
        }
        (self.start + Duration::minutes(self.duration_minutes))
            .format(if use_24h { "%H:%M" } else { "%I:%M %p" })
            .to_string()
    }
    pub fn timestamp(&self) -> Option<i64> {
        Local
            .from_local_datetime(&self.date.and_time(self.start))
            .single()
            .map(|x| x.timestamp())
    }
    pub fn occurs_on(&self, date: NaiveDate) -> bool {
        if self.all_day {
            date >= self.date
                && date < self.date + Duration::days((self.duration_minutes / 1440).max(1))
        } else {
            date == self.date
        }
    }
}

/// Demo events are visual examples. They never create scheduled reminders.
pub fn demo_events(today: NaiveDate) -> Vec<Event> {
    let mut events = Vec::new();
    for offset in -14..=62 {
        let date = today + Duration::days(offset);
        let templates = if offset == 0 {
            vec![
                (
                    9,
                    30,
                    "Un momento para enfocarte",
                    EventKind::Focus,
                    60,
                    false,
                ),
                (
                    11,
                    0,
                    "Ideas para el siguiente paso",
                    EventKind::Meet,
                    45,
                    true,
                ),
                (
                    16,
                    30,
                    "Café y buenas conversaciones",
                    EventKind::Place,
                    30,
                    false,
                ),
            ]
        } else if offset % 3 == 0 {
            vec![
                (10, 0, "Revisión de diseño", EventKind::Zoom, 45, false),
                (15, 0, "Leer, pensar, crear", EventKind::Focus, 90, false),
            ]
        } else if offset % 3 == 1 {
            vec![
                (11, 30, "Nos ponemos al día", EventKind::Teams, 30, false),
                (14, 0, "Notas e inspiración", EventKind::Link, 30, false),
            ]
        } else {
            vec![]
        };
        for (index, (hour, minute, title, kind, duration, important)) in
            templates.into_iter().enumerate()
        {
            events.push(Event {
                all_day: false,
                id: format!("demo-{date}-{index}"),
                title: title.into(),
                date,
                start: NaiveTime::from_hms_opt(hour, minute, 0).unwrap(),
                duration_minutes: duration,
                kind,
                calendar: if kind == EventKind::Focus {
                    "Personal"
                } else {
                    "Mi calendario"
                }
                .into(),
                description:
                    "Evento de ejemplo para revisar el calendario. No genera avisos automáticos."
                        .into(),
                destination: if kind == EventKind::Place {
                    "Tu cafetería favorita".into()
                } else {
                    String::new()
                },
                important,
                demo: true,
            });
        }
    }
    events
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub due_at: i64,
    pub important: bool,
    pub state: String,
    pub event_id: Option<String>,
}

/// Extract only the domain for display. Never replace the original click target.
pub fn web_domain(value: &str) -> Option<String> {
    static HOST: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^https?://(\[[0-9a-f:.]+\]|[^:/?#]+)").expect("valid domain regex")
    });
    let (_, authority) = value.split_once("://")?;
    if authority.is_empty()
        || authority.starts_with('/')
        || value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return None;
    }
    let parsed = url::Url::parse(value).ok()?;
    if !matches!(parsed.scheme(), "https" | "http")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.host_str().is_none()
    {
        return None;
    }
    let normalized = parsed.as_str();
    let mut captures = HOST.create_captures();
    HOST.captures(normalized, &mut captures);
    let host = captures.get_group(1)?;
    Some(normalized[host.start..host.end].to_owned())
}

pub fn valid_web_url(value: &str) -> bool {
    web_domain(value).is_some()
}

pub enum DescriptionPart<'a> {
    Text(&'a str),
    Link { domain: String, url: &'a str },
}

/// Keep prose intact while giving embedded URLs a compact, clickable label.
pub fn description_parts(value: &str) -> Vec<DescriptionPart<'_>> {
    static LINKS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i-u:https?)://[^ \t\r\n<>"\\]+"#).expect("valid description link regex")
    });
    let mut parts = Vec::new();
    let mut cursor = 0;
    for found in LINKS.find_iter(value) {
        let candidate = &value[found.start()..found.end()];
        let mut target = candidate.trim_end_matches(['.', ',', ';', '!', '?']);
        // Preserve balanced parentheses in URL paths, excluding surrounding prose.
        for (open, close) in [('(', ')'), ('[', ']')] {
            while target.ends_with(close)
                && target.matches(close).count() > target.matches(open).count()
            {
                target = &target[..target.len() - 1];
            }
        }
        if let Some(domain) = web_domain(target) {
            parts.push(DescriptionPart::Text(&value[cursor..found.start()]));
            parts.push(DescriptionPart::Link {
                domain,
                url: target,
            });
            cursor = found.start() + target.len();
        }
    }
    parts.push(DescriptionPart::Text(&value[cursor..]));
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_web_urls_can_be_opened() {
        for bad in [
            "file:///C:/secrets",
            "javascript:alert(1)",
            "https://",
            "https:///bad",
            "https://example.org\ncmd",
            "https://user@example.org",
            "https://example.org:invalid/",
            "https://example.org\\@other.org",
            "https://:secret@example.org",
        ] {
            assert!(!valid_web_url(bad));
        }
        assert!(valid_web_url("https://meet.google.com/abc-defg-hij"));
    }
    #[test]
    fn domain_hides_path_query_and_port_without_changing_the_link() {
        let target = format!(
            "https://us06web.zoom.us:443/w/123?tk={}#join",
            "x".repeat(2000)
        );
        assert_eq!(web_domain(&target).as_deref(), Some("us06web.zoom.us"));
        assert_eq!(
            web_domain("HTTPS://EXAMPLE.ORG/path").as_deref(),
            Some("example.org")
        );
        assert_eq!(
            web_domain("http://[::1]:8080/path").as_deref(),
            Some("[::1]")
        );
        assert!(target.ends_with("#join"));
    }
    #[test]
    fn embedded_links_preserve_prose_and_complete_click_targets() {
        let text =
            "Inicio (https://example.org/a_(b)?id=123). Sigue https://meet.google.com/a-b-c, fin.";
        let parts = description_parts(text);
        let mut reconstructed = String::new();
        let mut links = Vec::new();
        for part in parts {
            match part {
                DescriptionPart::Text(value) => reconstructed.push_str(value),
                DescriptionPart::Link { domain, url } => {
                    reconstructed.push_str(url);
                    links.push((domain, url));
                }
            }
        }
        assert_eq!(reconstructed, text);
        assert_eq!(
            links,
            vec![
                ("example.org".into(), "https://example.org/a_(b)?id=123"),
                ("meet.google.com".into(), "https://meet.google.com/a-b-c"),
            ]
        );
    }
    #[test]
    fn conferencing_locations_are_recognized_by_domain_boundaries() {
        let mut event = demo_events(Local::now().date_naive()).remove(0);
        event.kind = EventKind::Place;
        event.destination = "https://us06web.zoom.us/w/123?tk=demo".into();
        assert_eq!(event.display_kind(), EventKind::Zoom);
        event.destination = "https://zoom.us.example.org/".into();
        assert_eq!(event.display_kind(), EventKind::Link);
        event.destination = "Aula 302".into();
        assert_eq!(event.display_kind(), EventKind::Place);
    }
}
