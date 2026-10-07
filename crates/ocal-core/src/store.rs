use crate::*;
use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::Path;

pub struct Store {
    db: Connection,
}
impl Store {
    pub fn memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        let s = Self::init(Connection::open(path)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(s)
    }
    fn init(db: Connection) -> Result<Self> {
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
        CREATE TABLE IF NOT EXISTS accounts(id TEXT PRIMARY KEY,data TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS calendars(id TEXT PRIMARY KEY,data TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS events(id TEXT PRIMARY KEY,calendar TEXT NOT NULL REFERENCES calendars(id),data TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS calendar_sets(id TEXT PRIMARY KEY,data TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS preferences(key TEXT PRIMARY KEY,value TEXT NOT NULL);")?;
        let s = Self { db };
        if s.calendars()?.is_empty() {
            for (id, name, color) in [
                ("local-personal", "Personal", "#7aa2f7"),
                ("local-work", "Work", "#bb9af7"),
                ("local-tasks", "Tasks", "#9ece6a"),
            ] {
                s.save_calendar(&Calendar {
                    id: id.into(),
                    account: None,
                    remote_id: None,
                    name: name.into(),
                    color: color.into(),
                    timezone: "Europe/London".into(),
                    writable: true,
                    visible: true,
                    meet_available: false,
                })?;
            }
        }
        Ok(s)
    }
    pub fn save_calendar(&self, c: &Calendar) -> Result<()> {
        self.db.execute(
            "INSERT INTO calendars VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            params![c.id, serde_json::to_string(c)?],
        )?;
        Ok(())
    }
    pub fn save_account(&self, a: &Account) -> Result<()> {
        self.db.execute(
            "INSERT INTO accounts VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            params![a.id, serde_json::to_string(a)?],
        )?;
        Ok(())
    }
    pub fn save_event(&self, e: &Event) -> Result<()> {
        e.validate()?;
        self.db.execute("INSERT INTO events VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET calendar=excluded.calendar,data=excluded.data",params![e.id,e.calendar,serde_json::to_string(e)?])?;
        Ok(())
    }
    pub fn delete_event(&self, id: &str) -> Result<()> {
        self.db.execute("DELETE FROM events WHERE id=?1", [id])?;
        Ok(())
    }
    pub fn event(&self, id: &str) -> Result<Event> {
        let text: String = self
            .db
            .query_row("SELECT data FROM events WHERE id=?1", [id], |r| r.get(0))
            .context("This event is no longer available")?;
        Ok(serde_json::from_str(&text)?)
    }
    pub fn calendar(&self, id: &str) -> Result<Calendar> {
        let text: String =
            self.db
                .query_row("SELECT data FROM calendars WHERE id=?1", [id], |r| r.get(0))?;
        Ok(serde_json::from_str(&text)?)
    }
    pub fn calendars(&self) -> Result<Vec<Calendar>> {
        self.read("SELECT data FROM calendars ORDER BY id")
    }
    fn read<T: serde::de::DeserializeOwned>(&self, sql: &str) -> Result<Vec<T>> {
        let mut statement = self.db.prepare(sql)?;
        let rows = statement.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        Ok(Snapshot {
            calendars: self.calendars()?,
            accounts: self.read("SELECT data FROM accounts ORDER BY id")?,
            events: self.read("SELECT data FROM events")?,
            sets: self.read("SELECT data FROM calendar_sets ORDER BY id")?,
            preferences: self.preferences()?,
        })
    }
    pub fn preferences(&self) -> Result<std::collections::BTreeMap<String, String>> {
        let mut statement = self.db.prepare("SELECT key, value FROM preferences")?;
        let rows = statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
    pub fn delete_set(&self, id: &str) -> Result<()> {
        self.db
            .execute("DELETE FROM calendar_sets WHERE id=?1", [id])?;
        Ok(())
    }
    pub fn save_set(&self, set: &CalendarSet) -> Result<()> {
        self.db.execute("INSERT INTO calendar_sets VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",params![set.id,serde_json::to_string(set)?])?;
        Ok(())
    }
    pub fn save_preference(&self, key: &str, value: &str) -> Result<()> {
        self.db.execute("INSERT INTO preferences VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value])?;
        Ok(())
    }
    /// Replace a fully fetched calendar atomically. Failed pages never erase the cache.
    pub fn replace_remote(&mut self, calendar: &str, events: &[Event]) -> Result<()> {
        let transaction = self.db.transaction()?;
        transaction.execute("DELETE FROM events WHERE calendar=?1", [calendar])?;
        for e in events {
            transaction.execute(
                "INSERT INTO events VALUES(?1,?2,?3)",
                params![e.id, e.calendar, serde_json::to_string(e)?],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
    pub fn import(&mut self, events: &[Event]) -> Result<usize> {
        for e in events {
            e.validate()?;
        }
        let tx = self.db.transaction()?;
        for e in events {
            tx.execute("INSERT INTO events VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET calendar=excluded.calendar,data=excluded.data",params![e.id,e.calendar,serde_json::to_string(e)?])?;
        }
        tx.commit()?;
        Ok(events.len())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persistence_and_failed_import_are_atomic() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("db");
        let mut s = Store::open(&path).unwrap();
        let mut e = Event::new("local-personal", chrono::Utc::now().date_naive(), "UTC").unwrap();
        e.title = "Lunch".into();
        s.save_event(&e).unwrap();
        let mut bad = e.clone();
        bad.id = "bad".into();
        bad.title.clear();
        assert!(s.import(&[e.clone(), bad]).is_err());
        drop(s);
        let s = Store::open(&path).unwrap();
        assert_eq!(s.snapshot().unwrap().events.len(), 1);
        assert_eq!(s.event(&e.id).unwrap().title, "Lunch");
    }
    #[test]
    fn reconnecting_and_syncing_one_account_preserves_the_other_accounts_and_local_events() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("calendar.db");
        let mut store = Store::open(&path).unwrap();
        let mut ids = Vec::new();
        for account in ["first@example.com", "second@example.com"] {
            store
                .save_account(&Account {
                    id: account.into(),
                    name: account.into(),
                })
                .unwrap();
            let mut calendar = store.calendar("local-work").unwrap();
            calendar.id = format!("google:{account}:shared@example.com");
            calendar.account = Some(account.into());
            calendar.remote_id = Some("shared@example.com".into());
            store.save_calendar(&calendar).unwrap();
            let mut event =
                Event::new(&calendar.id, chrono::Utc::now().date_naive(), "UTC").unwrap();
            event.id = format!("{}:same-event", calendar.id);
            event.title = "Review".into();
            store.save_event(&event).unwrap();
            ids.push((calendar.id, event.id));
        }
        let mut local =
            Event::new("local-personal", chrono::Utc::now().date_naive(), "UTC").unwrap();
        local.title = "Local appointment".into();
        store.save_event(&local).unwrap();
        store
            .save_account(&Account {
                id: "first@example.com".into(),
                name: "Updated account".into(),
            })
            .unwrap();
        let mut changed = store.event(&ids[0].1).unwrap();
        changed.title = "Updated review".into();
        store.replace_remote(&ids[0].0, &[changed]).unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        let snapshot = store.snapshot().unwrap();
        assert_eq!(snapshot.accounts.len(), 2);
        assert_eq!(snapshot.calendars.len(), 5);
        assert_eq!(snapshot.events.len(), 3);
        assert_eq!(store.event(&ids[0].1).unwrap().title, "Updated review");
        assert_eq!(store.event(&ids[1].1).unwrap().title, "Review");
        assert_eq!(store.event(&local.id).unwrap().title, "Local appointment");
        assert_eq!(
            store.calendar(&ids[1].0).unwrap().account.as_deref(),
            Some("second@example.com")
        );
    }

    #[test]
    fn tasks_and_sets_round_trip() {
        let s = Store::memory().unwrap();
        let mut e = Event::new("local-tasks", chrono::Utc::now().date_naive(), "UTC").unwrap();
        e.title = "Review".into();
        e.task = true;
        e.completed = true;
        s.save_event(&e).unwrap();
        s.save_set(&CalendarSet {
            id: "focus".into(),
            name: "Focus".into(),
            calendars: vec!["local-work".into()],
        })
        .unwrap();
        let snapshot = s.snapshot().unwrap();
        assert!(snapshot.events[0].completed);
        assert_eq!(snapshot.sets[0].calendars, vec!["local-work"]);
        s.delete_set("focus").unwrap();
        assert!(s.snapshot().unwrap().sets.is_empty());
    }
    #[test]
    fn preferences_survive_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("calendar.sqlite3");
        {
            let s = Store::open(&path).unwrap();
            s.save_preference("view", "month").unwrap();
            s.save_preference("zoom", "1.2").unwrap();
            s.save_preference("view", "agenda").unwrap();
        }
        let prefs = Store::open(&path).unwrap().snapshot().unwrap().preferences;
        assert_eq!(prefs.get("view").map(String::as_str), Some("agenda"));
        assert_eq!(prefs.get("zoom").map(String::as_str), Some("1.2"));
    }
}
