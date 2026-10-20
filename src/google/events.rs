use crate::{
    model::{Event, EventKind, valid_web_url},
    reminders::{CalendarReminder, EventReminders, popup_times},
};
use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedEvent {
    pub event: Event,
    pub reminders: Vec<i64>,
    #[serde(default)]
    pub starts_at: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub account: String,
    pub calendar: String,
    pub synced_at: i64,
    pub from: NaiveDate,
    pub until: NaiveDate,
    pub events: Vec<SyncedEvent>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarList {
    #[serde(default)]
    items: Vec<CalendarInfo>,
    next_page_token: Option<String>,
}
#[derive(Deserialize)]
struct CalendarInfo {
    id: String,
    #[serde(default)]
    primary: bool,
}

pub(super) async fn account(http: &reqwest::Client, token: &str) -> Result<String, String> {
    let mut page = String::new();
    for _ in 0..100 {
        let mut request = http
            .get("https://www.googleapis.com/calendar/v3/users/me/calendarList")
            .bearer_auth(token)
            .query(&[("maxResults", "250")]);
        if !page.is_empty() {
            request = request.query(&[("pageToken", page.as_str())]);
        }
        let result = request.send().await.map_err(|_| {
            "No se pudo consultar tu cuenta. Revisa la conexión a Internet.".to_string()
        })?;
        let data: CalendarList = read_response(result).await?;
        if let Some(primary) = data.items.into_iter().find(|c| c.primary) {
            return Ok(primary.id);
        }
        match data.next_page_token {
            Some(next) => page = next,
            None => break,
        }
    }
    Err("No se encontró el calendario principal de esta cuenta.".into())
}
async fn read_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, String> {
    match response.status().as_u16(){
        200..=299=>response.json().await.map_err(|_|"Google devolvió datos que no se pudieron leer.".into()),
        401=>Err("La autorización de Google venció o fue retirada. Desconecta y vuelve a conectar la cuenta.".into()),
        403=>Err("Google no permite leer el calendario. Comprueba el acceso concedido o inténtalo más tarde.".into()),
        429=>Err("Google ha limitado temporalmente las consultas. Se intentará de nuevo más tarde.".into()),
        _=>Err("No se pudo actualizar Google Calendar. Tus últimos datos siguen disponibles.".into()),
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventsPage {
    #[serde(default)]
    summary: String,
    #[serde(default = "default_zone")]
    time_zone: String,
    #[serde(default)]
    default_reminders: Vec<CalendarReminder>,
    #[serde(default)]
    items: Vec<RemoteEvent>,
    next_page_token: Option<String>,
}
fn default_zone() -> String {
    "UTC".into()
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct EventTime {
    date_time: Option<String>,
    date: Option<NaiveDate>,
    time_zone: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteEvent {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    html_link: String,
    #[serde(default)]
    hangout_link: String,
    #[serde(default)]
    start: EventTime,
    #[serde(default)]
    end: EventTime,
    reminders: Option<EventReminders>,
    #[serde(default)]
    attendees: Vec<Attendee>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Attendee {
    #[serde(default, rename = "self")]
    self_: bool,
    #[serde(default)]
    response_status: String,
}

pub(super) async fn fetch(
    http: &reqwest::Client,
    token: &str,
    now: DateTime<Local>,
) -> Result<Snapshot, String> {
    let from = (now - Duration::days(7)).date_naive();
    let until = (now + Duration::days(90)).date_naive();
    let mut snapshot = Snapshot {
        account: String::new(),
        calendar: String::new(),
        synced_at: now.timestamp(),
        from,
        until,
        events: Vec::new(),
    };
    let account = account(http, token).await?;
    snapshot.account = account;
    let mut page = String::new();
    for _ in 0..100 {
        let mut request = http
            .get("https://www.googleapis.com/calendar/v3/calendars/primary/events")
            .bearer_auth(token)
            .query(&[
                ("singleEvents", "true"),
                ("orderBy", "startTime"),
                ("maxResults", "2500"),
                ("timeMin", &(now - Duration::days(7)).to_rfc3339()),
                ("timeMax", &(now + Duration::days(90)).to_rfc3339()),
            ]);
        if !page.is_empty() {
            request = request.query(&[("pageToken", page.as_str())]);
        }
        let response = request.send().await.map_err(|_| {
            "No se pudo actualizar el calendario. Revisa la conexión a Internet.".to_string()
        })?;
        let data: EventsPage = read_response(response).await?;
        snapshot.calendar = data.summary.clone();
        for raw in data.items {
            if let Some(event) = convert(
                raw,
                &snapshot.account,
                &data.summary,
                &data.time_zone,
                &data.default_reminders,
            )? {
                snapshot.events.push(event);
            }
        }
        match data.next_page_token {
            Some(next) => page = next,
            None => {
                snapshot
                    .events
                    .sort_by_key(|e| (e.event.date, e.event.start));
                return Ok(snapshot);
            }
        }
    }
    Err("El calendario es demasiado grande para esta actualización. Se conservaron los datos anteriores.".into())
}
fn convert(
    raw: RemoteEvent,
    account: &str,
    calendar: &str,
    zone: &str,
    defaults: &[CalendarReminder],
) -> Result<Option<SyncedEvent>, String> {
    if raw.status == "cancelled"
        || raw
            .attendees
            .iter()
            .any(|a| a.self_ && a.response_status == "declined")
    {
        return Ok(None);
    }
    let all_day = raw.start.date.is_some();
    let (start, end) = if let Some(ref value) = raw.start.date_time {
        let start = DateTime::parse_from_rfc3339(value)
            .map_err(|_| "No se pudo interpretar el horario de un evento de Google.".to_string())?
            .timestamp();
        let end = raw
            .end
            .date_time
            .as_deref()
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.timestamp())
            .unwrap_or(start);
        (start, end)
    } else if let Some(date) = raw.start.date {
        let zone: chrono_tz::Tz = raw
            .start
            .time_zone
            .as_deref()
            .unwrap_or(zone)
            .parse()
            .map_err(|_| "Google devolvió una zona horaria desconocida.".to_string())?;
        let start = zone
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .ok_or("No se pudo interpretar el día del evento.")?
            .timestamp();
        let end_date = raw.end.date.unwrap_or(date + Duration::days(1));
        // Duration is calendar days for all-day display, independent of DST.
        (start, start + (end_date - date).num_days() * 86_400)
    } else {
        return Ok(None);
    };
    let local = DateTime::from_timestamp(start, 0)
        .ok_or("La fecha del evento no es válida.")?
        .with_timezone(&Local);
    let (kind, destination) = if valid_web_url(&raw.hangout_link) {
        (EventKind::Meet, raw.hangout_link)
    } else if !raw.location.is_empty() {
        (EventKind::Place, raw.location)
    } else if valid_web_url(&raw.html_link) {
        (EventKind::Link, raw.html_link)
    } else {
        (EventKind::Focus, String::new())
    };
    let policy = raw.reminders.unwrap_or(EventReminders {
        use_default: true,
        overrides: Vec::new(),
    });
    let reminders = popup_times(start, &policy, defaults);
    let event = Event {
        id: format!("google:{account}:{}", raw.id),
        title: if raw.summary.is_empty() {
            "Sin título".into()
        } else {
            raw.summary
        },
        date: if all_day {
            raw.start.date.unwrap()
        } else {
            local.date_naive()
        },
        start: if all_day {
            chrono::NaiveTime::MIN
        } else {
            local.time()
        },
        duration_minutes: ((end - start) / 60).max(0),
        kind,
        calendar: calendar.into(),
        description: plain_text(&raw.description),
        destination,
        important: false,
        demo: false,
        all_day,
    };
    Ok(Some(SyncedEvent {
        event,
        reminders,
        starts_at: Some(start),
    }))
}
fn plain_text(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                if !text.ends_with([' ', '\n']) {
                    text.push(' ');
                }
            }
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
        .trim()
        .into()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn raw(value: serde_json::Value) -> RemoteEvent {
        serde_json::from_value(value).unwrap()
    }
    #[test]
    fn converts_absolute_event_time_and_uses_remote_defaults() {
        let r = raw(
            serde_json::json!({"id":"event","summary":"Meeting","start":{"dateTime":"2026-10-20T10:00:00-05:00"},"end":{"dateTime":"2026-10-20T11:00:00-05:00"},"hangoutLink":"https://meet.google.com/abc-defg-hij","reminders":{"useDefault":true}}),
        );
        let event = convert(
            r,
            "person@example.org",
            "Personal",
            "America/Lima",
            &[CalendarReminder {
                method: "popup".into(),
                minutes: 7,
            }],
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            event.reminders,
            vec![
                DateTime::parse_from_rfc3339("2026-10-20T09:53:00-05:00")
                    .unwrap()
                    .timestamp()
            ]
        );
        assert_eq!(event.event.kind, EventKind::Meet);
        assert_eq!(
            event.starts_at,
            Some(
                DateTime::parse_from_rfc3339("2026-10-20T10:00:00-05:00")
                    .unwrap()
                    .timestamp()
            )
        );
        assert!(!event.event.demo);
    }
    #[test]
    fn cancellation_and_declined_invitations_never_schedule() {
        assert!(
            convert(
                raw(serde_json::json!({"id":"gone","status":"cancelled"})),
                "a",
                "c",
                "UTC",
                &[]
            )
            .unwrap()
            .is_none()
        );
        assert!(convert(raw(serde_json::json!({"id":"no","attendees":[{"self":true,"responseStatus":"declined"}]})),"a","c","UTC",&[]).unwrap().is_none());
    }
    #[test]
    fn all_day_uses_calendar_zone_and_keeps_the_original_date() {
        let r = raw(
            serde_json::json!({"id":"day","start":{"date":"2026-10-20"},"end":{"date":"2026-10-22"},"reminders":{"useDefault":false,"overrides":[{"method":"popup","minutes":0}]}}),
        );
        let result = convert(r, "a", "c", "Asia/Tokyo", &[]).unwrap().unwrap();
        assert_eq!(
            result.reminders[0],
            DateTime::parse_from_rfc3339("2026-10-20T00:00:00+09:00")
                .unwrap()
                .timestamp()
        );
        assert!(result.event.all_day);
        assert_eq!(result.event.date.to_string(), "2026-10-20");
        assert_eq!(result.event.duration_minutes, 2880);
    }
}
