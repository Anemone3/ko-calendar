use crate::model::{Event, Reminder};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::{error::Error, path::Path};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub struct Store {
    connection: Connection,
}
pub struct AdvanceAlarm {
    pub id: i64,
    pub event_id: String,
    pub due_at: i64,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(3))?;
        // One resident owns this database. Rollback journaling needs no shared WAL files.
        // SQLite checkpoints any previous WAL before switching modes.
        connection.pragma_update(None, "journal_mode", "DELETE")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        Self::initialize(connection)
    }

    fn initialize(mut connection: Connection) -> Result<Self> {
        connection.pragma_update(None, "foreign_keys", true)?;
        let tx = connection.transaction()?;
        tx.execute_batch("
            CREATE TABLE IF NOT EXISTS settings (id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS events (id TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS event_flags (id TEXT PRIMARY KEY, important INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS reminders (
                id INTEGER PRIMARY KEY, title TEXT NOT NULL, body TEXT NOT NULL,
                due_at INTEGER NOT NULL, important INTEGER NOT NULL,
                state TEXT NOT NULL CHECK(state IN ('pending','delivered','dismissed','expired')),
                event_id TEXT
            );
            CREATE INDEX IF NOT EXISTS reminders_due ON reminders(state, due_at);

        ")?;
        // Keep the previous prototype's data, but never replay its local offsets or snoozes.
        let has_origin = tx
            .prepare("PRAGMA table_info(reminders)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .iter()
            .any(|name| name == "origin");
        if !has_origin {
            tx.execute(
                "ALTER TABLE reminders ADD COLUMN origin TEXT NOT NULL DEFAULT 'legacy'",
                [],
            )?;
        }
        tx.execute_batch("CREATE TABLE IF NOT EXISTS google_cache(id INTEGER PRIMARY KEY CHECK(id=1),data TEXT NOT NULL); CREATE UNIQUE INDEX IF NOT EXISTS calendar_reminder_identity ON reminders(event_id,due_at) WHERE origin='calendar';")?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS sound_settings(id INTEGER PRIMARY KEY CHECK(id=1),data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS alarm_schedule(event_id TEXT NOT NULL,starts_at INTEGER NOT NULL,due_at INTEGER NOT NULL,state TEXT NOT NULL CHECK(state IN ('pending','delivered','dismissed','expired')),PRIMARY KEY(event_id,due_at));
            CREATE INDEX IF NOT EXISTS alarm_due ON alarm_schedule(state,due_at);")?;
        if tx.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))? < 4 {
            tx.pragma_update(None, "user_version", 4)?;
        }
        tx.commit()?;
        Ok(Self { connection })
    }

    pub fn local_events(&self) -> Result<Vec<Event>> {
        let mut stmt = self
            .connection
            .prepare("SELECT data FROM events ORDER BY id")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }

    pub fn sound_settings(&self) -> Result<crate::sound_settings::SoundSettings> {
        let data: Option<String> = self
            .connection
            .query_row("SELECT data FROM sound_settings WHERE id=1", [], |row| {
                row.get(0)
            })
            .optional()?;
        let mut settings: crate::sound_settings::SoundSettings = data
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default();
        settings.volume = settings.volume.min(100);
        Ok(settings)
    }

    pub fn save_sound_settings(
        &self,
        settings: &crate::sound_settings::SoundSettings,
    ) -> Result<()> {
        self.connection.execute("INSERT INTO sound_settings(id,data) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET data=excluded.data", [serde_json::to_string(settings)?])?;
        Ok(())
    }

    pub fn reconcile_alarms(&mut self, snapshot: &crate::google::Snapshot, now: i64) -> Result<()> {
        let tx = self.connection.transaction()?;
        reconcile_alarms(&tx, snapshot, now)?;
        tx.commit()?;
        Ok(())
    }

    /// Claim once, coalescing simultaneous events. Never replay an alarm after its event starts.
    pub fn claim_alarms(&mut self, now: i64) -> Result<Vec<AdvanceAlarm>> {
        let tx = self.connection.transaction()?;
        tx.execute("UPDATE alarm_schedule SET state='expired' WHERE state='pending' AND (due_at < ?1 OR starts_at <= ?2)", params![now - 60, now])?;
        let alarms = {
            let mut stmt = tx.prepare("SELECT rowid,event_id,due_at FROM alarm_schedule WHERE state='pending' AND due_at <= ?1 ORDER BY due_at,event_id")?;
            stmt.query_map([now], |r| {
                Ok(AdvanceAlarm {
                    id: r.get(0)?,
                    event_id: r.get(1)?,
                    due_at: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        tx.execute(
            "UPDATE alarm_schedule SET state='delivered' WHERE state='pending' AND due_at <= ?1",
            [now],
        )?;
        tx.commit()?;
        Ok(alarms)
    }

    pub fn google_snapshot(&self) -> Result<Option<crate::google::Snapshot>> {
        let mut statement = self
            .connection
            .prepare("SELECT data FROM google_cache WHERE id=1")?;
        let mut rows = statement.query([])?;
        rows.next()?
            .map(|row| Ok(serde_json::from_str(&row.get::<_, String>(0)?)?))
            .transpose()
    }
    /// Apply a complete successful fetch atomically. Failed or partial requests never erase cache.
    pub fn apply_google(&mut self, snapshot: &crate::google::Snapshot, now: i64) -> Result<()> {
        let tx = self.connection.transaction()?;
        let previous = {
            let mut statement = tx.prepare(
                "SELECT event_id,due_at FROM reminders WHERE origin='calendar' AND state='pending'",
            )?;
            statement
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
                .collect::<rusqlite::Result<std::collections::HashSet<_>>>()?
        };
        tx.execute(
            "UPDATE reminders SET state='dismissed' WHERE origin='calendar' AND state='pending'",
            [],
        )?;
        for item in &snapshot.events {
            for due in &item.reminders {
                if *due < now
                    && (!previous.contains(&(item.event.id.clone(), *due)) || *due < now - 86_400)
                {
                    continue;
                }
                tx.execute("INSERT INTO reminders(title,body,due_at,important,state,event_id,origin) VALUES(?1,?2,?3,0,'pending',?4,'calendar') ON CONFLICT(event_id,due_at) WHERE origin='calendar' DO UPDATE SET title=excluded.title,body=excluded.body,state=CASE WHEN reminders.state='delivered' THEN 'delivered' ELSE 'pending' END",params![item.event.title,format!("{} · {}",item.event.time_label(true),snapshot.calendar),due,item.event.id])?;
            }
        }
        tx.execute("INSERT INTO google_cache(id,data) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET data=excluded.data",[serde_json::to_string(snapshot)?])?;
        reconcile_alarms(&tx, snapshot, now)?;
        tx.commit()?;
        Ok(())
    }
    pub fn clear_google(&mut self) -> Result<()> {
        let tx = self.connection.transaction()?;
        tx.execute("DELETE FROM google_cache", [])?;
        tx.execute("DELETE FROM reminders WHERE origin='calendar'", [])?;
        tx.execute("DELETE FROM alarm_schedule", [])?;
        tx.commit()?;
        Ok(())
    }

    /// Only the explicit tray test uses this path; event reminders come from the calendar.
    pub fn schedule_test(
        &self,
        title: &str,
        body: &str,
        due_at: i64,
        important: bool,
        event_id: Option<&str>,
    ) -> Result<i64> {
        self.connection.execute("INSERT INTO reminders(title,body,due_at,important,state,event_id,origin) VALUES(?1,?2,?3,?4,'pending',?5,'test')", params![title, body, due_at, important, event_id])?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn next_due(&self) -> Result<Option<i64>> {
        Ok(self.connection.query_row(
            "SELECT MIN(due_at) FROM (SELECT due_at FROM reminders WHERE state='pending' AND origin IN ('calendar','test') UNION ALL SELECT due_at FROM alarm_schedule WHERE state='pending')",
            [],
            |row| row.get(0),
        )?)
    }

    /// Claim before delivery so restarting the app never repeats an emitted reminder.
    /// Very old reminders expire; a resume does not replay days of stale alarms.
    pub fn claim_due(&mut self, now: i64) -> Result<Vec<Reminder>> {
        let tx = self.connection.transaction()?;
        tx.execute(
            "UPDATE reminders SET state='expired' WHERE state='pending' AND due_at < ?1",
            [now - 86_400],
        )?;
        let due = {
            let mut stmt = tx.prepare("SELECT id,title,body,due_at,important,state,event_id FROM reminders WHERE state='pending' AND origin IN ('calendar','test') AND due_at<=?1 ORDER BY due_at,id LIMIT 32")?;
            stmt.query_map([now], read_reminder)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for reminder in &due {
            tx.execute(
                "UPDATE reminders SET state='delivered' WHERE id=?1 AND state='pending'",
                [reminder.id],
            )?;
        }
        tx.commit()?;
        Ok(due
            .into_iter()
            .map(|mut r| {
                r.state = "delivered".into();
                r
            })
            .collect())
    }

    pub fn recent_reminders(&self) -> Result<Vec<Reminder>> {
        let mut stmt = self.connection.prepare("SELECT id,title,body,due_at,important,state,event_id FROM reminders WHERE state IN ('pending','delivered') ORDER BY CASE state WHEN 'delivered' THEN 0 ELSE 1 END,due_at LIMIT 32")?;
        Ok(stmt
            .query_map([], read_reminder)?
            .collect::<rusqlite::Result<_>>()?)
    }
}

fn reconcile_alarms(
    tx: &Transaction<'_>,
    snapshot: &crate::google::Snapshot,
    now: i64,
) -> Result<()> {
    let previous = {
        let mut stmt =
            tx.prepare("SELECT event_id,due_at FROM alarm_schedule WHERE state='pending'")?;
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<rusqlite::Result<std::collections::HashSet<_>>>()?
    };
    tx.execute(
        "UPDATE alarm_schedule SET state='dismissed' WHERE state='pending'",
        [],
    )?;
    for item in &snapshot.events {
        if item.event.demo || item.event.all_day {
            continue;
        }
        let Some(start) = item.starts_at.or_else(|| item.event.timestamp()) else {
            continue;
        };
        let Some(due) = start.checked_sub(crate::sound_settings::ADVANCE_SECONDS) else {
            continue;
        };
        if start <= now
            || (due < now && (!previous.contains(&(item.event.id.clone(), due)) || due < now - 60))
        {
            continue;
        }
        tx.execute("INSERT INTO alarm_schedule(event_id,starts_at,due_at,state) VALUES(?1,?2,?3,'pending') ON CONFLICT(event_id,due_at) DO UPDATE SET starts_at=excluded.starts_at,state=CASE WHEN alarm_schedule.state='delivered' THEN 'delivered' ELSE 'pending' END", params![item.event.id,start,due])?;
    }
    Ok(())
}

fn read_reminder(row: &rusqlite::Row<'_>) -> rusqlite::Result<Reminder> {
    Ok(Reminder {
        id: row.get(0)?,
        title: row.get(1)?,
        body: row.get(2)?,
        due_at: row.get(3)?,
        important: row.get(4)?,
        state: row.get(5)?,
        event_id: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn memory() -> Store {
        Store::initialize(Connection::open_in_memory().unwrap()).unwrap()
    }
    fn alarm_snapshot(start: i64) -> crate::google::Snapshot {
        let today = chrono::Local::now().date_naive();
        let mut event = crate::model::demo_events(today).remove(0);
        event.id = "google:account:alarm".into();
        event.demo = false;
        crate::google::Snapshot {
            account: "account".into(),
            calendar: "Main".into(),
            synced_at: 600,
            from: today,
            until: today,
            events: vec![crate::google::events::SyncedEvent {
                event,
                reminders: vec![650],
                starts_at: Some(start),
            }],
        }
    }
    #[test]
    fn alarm_is_two_minutes_before_start_independent_of_google_banner() {
        let mut store = memory();
        let snapshot = alarm_snapshot(1000);
        store.apply_google(&snapshot, 600).unwrap();
        assert_eq!(store.next_due().unwrap(), Some(650));
        assert!(store.claim_alarms(650).unwrap().is_empty());
        assert_eq!(store.claim_due(650).unwrap().len(), 1);
        assert_eq!(store.next_due().unwrap(), Some(880));
        assert!(store.claim_alarms(879).unwrap().is_empty());
        assert!(!store.claim_alarms(880).unwrap().is_empty());
        assert!(store.claim_alarms(880).unwrap().is_empty());
        store.apply_google(&snapshot, 881).unwrap();
        assert!(store.claim_alarms(881).unwrap().is_empty());
        assert_eq!(store.next_due().unwrap(), None);
    }
    #[test]
    fn alarm_moves_cancels_and_never_replays_stale_or_all_day_events() {
        let mut store = memory();
        let mut snapshot = alarm_snapshot(1000);
        store.apply_google(&snapshot, 600).unwrap();
        snapshot.events[0].starts_at = Some(1200);
        store.apply_google(&snapshot, 620).unwrap();
        assert!(store.claim_alarms(880).unwrap().is_empty());
        assert!(!store.claim_alarms(1080).unwrap().is_empty());
        snapshot.events[0].starts_at = Some(1800);
        store.apply_google(&snapshot, 1130).unwrap();
        snapshot.events.clear();
        store.apply_google(&snapshot, 1180).unwrap();
        assert!(store.claim_alarms(1680).unwrap().is_empty());
        let mut all_day = alarm_snapshot(2000);
        all_day.events[0].event.all_day = true;
        store.apply_google(&all_day, 1700).unwrap();
        assert!(store.claim_alarms(1880).unwrap().is_empty());
        store.apply_google(&alarm_snapshot(3000), 2600).unwrap();
        assert!(store.claim_alarms(2941).unwrap().is_empty());
        // First sync after an alarm's deadline must not invent a late alarm.
        store.apply_google(&alarm_snapshot(4000), 3930).unwrap();
        assert!(store.claim_alarms(3930).unwrap().is_empty());
    }
    #[test]
    fn alarm_delivery_and_sound_preferences_survive_restart() {
        let path = std::env::temp_dir().join(format!(
            "kokonut-sound-{}-{}.sqlite3",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let settings = crate::sound_settings::SoundSettings {
            enabled: false,
            file: "mi alarma.mp3".into(),
            volume: 25,
            advance_enabled: true,
            advance_file: "anticipado.mp3".into(),
        };
        let snapshot = alarm_snapshot(1000);
        {
            let mut store = Store::open(&path).unwrap();
            store.save_sound_settings(&settings).unwrap();
            store.apply_google(&snapshot, 600).unwrap();
            // Muted alarms are still claimed, so enabling later does not replay them.
            assert!(!store.claim_alarms(880).unwrap().is_empty());
        }
        {
            let mut store = Store::open(&path).unwrap();
            assert_eq!(store.sound_settings().unwrap(), settings);
            store.reconcile_alarms(&snapshot, 881).unwrap();
            assert!(store.claim_alarms(881).unwrap().is_empty());
            store.clear_google().unwrap();
            assert_eq!(store.next_due().unwrap(), None);
        }
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn simultaneous_reminders_deliver_once() {
        let mut store = memory();
        store.schedule_test("A", "", 100, false, None).unwrap();
        store.schedule_test("B", "", 100, false, None).unwrap();
        assert!(store.claim_due(99).unwrap().is_empty());
        assert_eq!(store.claim_due(100).unwrap().len(), 2);
        assert!(store.claim_due(101).unwrap().is_empty());
        assert_eq!(store.next_due().unwrap(), None);
    }
    #[test]
    fn stale_reminders_expire_without_replaying() {
        let mut store = memory();
        store.schedule_test("Old", "", 1, false, None).unwrap();
        store
            .schedule_test("Relevant", "", 100_000, false, None)
            .unwrap();
        let reminders = store.claim_due(100_010).unwrap();
        assert_eq!(reminders.len(), 1);
        assert_eq!(reminders[0].title, "Relevant");
        assert_eq!(store.next_due().unwrap(), None);
    }
    #[test]
    fn migration_preserves_data_without_firing_old_local_offsets() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE reminders(id INTEGER PRIMARY KEY,title TEXT NOT NULL,body TEXT NOT NULL,due_at INTEGER NOT NULL,important INTEGER NOT NULL,state TEXT NOT NULL,event_id TEXT); INSERT INTO reminders VALUES(1,'Legacy','',100,0,'pending','local-1');").unwrap();
        let mut store = Store::initialize(connection).unwrap();
        assert_eq!(store.recent_reminders().unwrap().len(), 1);
        assert_eq!(store.next_due().unwrap(), None);
        assert!(store.claim_due(100).unwrap().is_empty());
        store
            .schedule_test("New test", "", 101, false, None)
            .unwrap();
        assert_eq!(store.claim_due(101).unwrap()[0].title, "New test");
    }
    #[test]
    fn delivered_state_survives_reopen() {
        let path = std::env::temp_dir().join(format!(
            "kokonut-test-{}-{}.sqlite3",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        {
            let mut store = Store::open(&path).unwrap();
            store
                .schedule_test("Persistent", "", 100, false, None)
                .unwrap();
            assert_eq!(store.claim_due(100).unwrap().len(), 1);
        }
        {
            let mut store = Store::open(&path).unwrap();
            assert!(store.claim_due(101).unwrap().is_empty());
        }
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn google_refresh_reconciles_changes_without_duplicates_or_old_alerts() {
        use crate::google::{Snapshot, events::SyncedEvent};
        let mut store = memory();
        let today = chrono::Local::now().date_naive();
        let mut event = crate::model::demo_events(today).remove(0);
        event.id = "google:account:instance".into();
        event.demo = false;
        let mut snapshot = Snapshot {
            account: "account".into(),
            calendar: "Main".into(),
            synced_at: 90,
            from: today,
            until: today,
            events: vec![SyncedEvent {
                event,
                reminders: vec![50, 100],
                starts_at: None,
            }],
        };
        store.apply_google(&snapshot, 90).unwrap();
        assert_eq!(store.next_due().unwrap(), Some(100));
        store.apply_google(&snapshot, 95).unwrap();
        assert_eq!(store.claim_due(100).unwrap().len(), 1);
        store.apply_google(&snapshot, 100).unwrap();
        assert!(store.claim_due(100).unwrap().is_empty());
        snapshot.events[0].reminders = vec![200];
        store.apply_google(&snapshot, 101).unwrap();
        assert_eq!(store.next_due().unwrap(), Some(200));
        snapshot.events.clear();
        store.apply_google(&snapshot, 150).unwrap();
        assert_eq!(store.next_due().unwrap(), None);
        assert!(store.google_snapshot().unwrap().unwrap().events.is_empty());
        store.clear_google().unwrap();
        assert!(store.google_snapshot().unwrap().is_none());
    }

    #[test]
    fn google_cache_and_delivered_reminders_survive_wal_migration_and_restarts() {
        use crate::google::{Snapshot, events::SyncedEvent};
        let path = std::env::temp_dir().join(format!(
            "kokonut-google-restart-{}-{}.sqlite3",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let today = chrono::Local::now().date_naive();
        let mut event = crate::model::demo_events(today).remove(0);
        event.id = "google:account:disk-event".into();
        event.demo = false;
        let snapshot = Snapshot {
            account: "account".into(),
            calendar: "Main".into(),
            synced_at: 90,
            from: today,
            until: today,
            events: vec![SyncedEvent {
                event,
                reminders: vec![100],
                starts_at: None,
            }],
        };
        {
            let mut store = Store::open(&path).unwrap();
            // Reproduce an existing installation's journal mode and stored delivery state.
            store
                .connection
                .pragma_update(None, "journal_mode", "WAL")
                .unwrap();
            store.apply_google(&snapshot, 90).unwrap();
            assert_eq!(store.claim_due(100).unwrap().len(), 1);
        }
        for now in [101, 102, 103] {
            let mut store = Store::open(&path).unwrap();
            let mode: String = store
                .connection
                .pragma_query_value(None, "journal_mode", |row| row.get(0))
                .unwrap();
            assert_eq!(mode, "delete");
            assert_eq!(
                store.google_snapshot().unwrap().unwrap().events[0].event.id,
                snapshot.events[0].event.id
            );
            store.apply_google(&snapshot, now).unwrap();
            assert!(store.claim_due(now).unwrap().is_empty());
            let integrity: String = store
                .connection
                .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                .unwrap();
            assert_eq!(integrity, "ok");
        }
        std::fs::remove_file(path).unwrap();
    }
}
