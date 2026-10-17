use crate::*;
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Duration, NaiveDate, Utc};
pub struct Service {
    pub store: Store,
    pub demo: bool,
    pub range: (NaiveDate, NaiveDate),
}
impl Service {
    pub fn new(store: Store, demo: bool, today: NaiveDate) -> Self {
        Self {
            store,
            demo,
            range: (today - Duration::days(60), today + Duration::days(120)),
        }
    }
    pub fn connect(
        &mut self,
        config: oauth::ClientConfig,
        progress: impl Fn(&str),
        link_ready: impl FnOnce(&str),
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<()> {
        ensure!(!self.demo, "Demo calendars cannot connect to Google");
        let tokens = oauth::authorize(config, link_ready, cancelled)?;
        progress("Google approved access · Loading your calendars…");
        let google = google::Google::from_tokens(&tokens)?;
        let id = google
            .primary_id()
            .context("Google sign-in succeeded, but Ocal could not load your calendars")?;
        progress("Saving this account to your desktop keyring…");
        oauth::store(&id, &tokens)?;
        self.store.save_account(&Account {
            id: id.clone(),
            name: id.clone(),
        })?;
        let mut calendars = google.calendars(&id)?;
        for c in &mut calendars {
            if let Ok(old) = self.store.calendar(&c.id) {
                c.visible = old.visible;
            }
            self.store.save_calendar(c)?;
        }
        progress("Google Calendar connected · Syncing your events…");
        // Connecting one account must not depend on another account's credentials.
        self.sync_account(&id)?;
        Ok(())
    }
    pub fn sync(&mut self) -> Result<()> {
        if self.demo {
            return Ok(());
        }
        let accounts = self.store.snapshot()?.accounts;
        sync_accounts(&accounts, |account| self.sync_account(&account.id))
    }
    fn sync_account(&mut self, account: &str) -> Result<()> {
        let google = google::Google::account(account)?;
        for mut calendar in google.calendars(account)? {
            if let Ok(old) = self.store.calendar(&calendar.id) {
                calendar.visible = old.visible;
            }
            self.store.save_calendar(&calendar)?;
            let events = google.events(&calendar, self.range.0, self.range.1)?;
            self.store.replace_remote(&calendar.id, &events)?;
        }
        Ok(())
    }
    pub fn save(&mut self, e: &Event, notify: bool) -> Result<Event> {
        let c = self.store.calendar(&e.calendar)?;
        ensure!(c.writable, "This calendar is read-only");
        ensure!(
            e.meet_request.is_none() || c.meet_available,
            "This calendar cannot generate Google Meet links"
        );
        if let Some(account) = &c.account {
            ensure!(!self.demo, "Demo calendars cannot write to Google");
            let saved = google::Google::account(account)?.save(e, &c, notify)?;
            self.store.save_event(&saved)?;
            Ok(saved)
        } else {
            self.store.save_event(e)?;
            Ok(e.clone())
        }
    }
    pub fn refresh(&self, e: &Event) -> Result<Event> {
        ensure!(!self.demo, "Demo has no live conferences");
        let c = self.store.calendar(&e.calendar)?;
        let saved = google::Google::account(c.account.as_deref().context("Local event")?)?
            .refresh(e, &c)?;
        self.store.save_event(&saved)?;
        Ok(saved)
    }
    pub fn delete(&mut self, e: &Event) -> Result<()> {
        let c = self.store.calendar(&e.calendar)?;
        ensure!(c.writable, "This calendar is read-only");
        if let Some(account) = &c.account {
            ensure!(!self.demo, "Demo calendars cannot write to Google");
            google::Google::account(account)?.delete(e, &c)?;
        }
        self.store.delete_event(&e.id)?;
        Ok(())
    }
    pub fn respond(&mut self, e: &Event, response: &str) -> Result<()> {
        ensure!(!self.demo, "Demo has no live invitations");
        let c = self.store.calendar(&e.calendar)?;
        let account = c.account.as_deref().context("This is a local event")?;
        let e = google::Google::account(account)?.respond(e, &c, response)?;
        self.store.save_event(&e)
    }
}

/// Report failing accounts after every account has had a chance to update its cache.
fn sync_accounts(accounts: &[Account], mut sync: impl FnMut(&Account) -> Result<()>) -> Result<()> {
    let mut failures = Vec::new();
    let mut offline = true;
    for account in accounts {
        if let Err(error) = sync(account) {
            offline &= is_offline(&error);
            failures.push(format!("{}: {error:#}", account.id));
        }
    }
    // The per-account text would hide that the network, not an account, failed.
    if !failures.is_empty() && offline {
        return Err(Offline.into());
    }
    ensure!(
        failures.is_empty(),
        "Some Google accounts could not sync:\n{}",
        failures.join("\n")
    );
    Ok(())
}

/// Google could not be reached at all.
#[derive(Debug)]
pub struct Offline;
impl std::fmt::Display for Offline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Google Calendar can’t be reached. Check your connection")
    }
}
impl std::error::Error for Offline {}

/// Whether an error means there is no connection, rather than a problem with an account.
pub fn is_offline(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.is::<Offline>()
            || cause
                .downcast_ref::<reqwest::Error>()
                .is_some_and(|e| e.is_connect() || e.is_timeout())
    })
}

/// How syncing with Google is going, for the sidebar footer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncState {
    pub syncing: bool,
    /// When a sync last finished without errors.
    pub last: Option<DateTime<Utc>>,
    /// Why the latest sync failed; the next good one clears it.
    pub error: Option<String>,
    /// The latest failure was a missing connection.
    pub offline: bool,
}
impl SyncState {
    pub fn finish(&mut self, result: &Result<()>, now: DateTime<Utc>) {
        self.syncing = false;
        match result {
            Ok(()) => {
                self.last = Some(now);
                self.error = None;
                self.offline = false;
            }
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                self.offline = is_offline(error);
            }
        }
    }
    /// "Synced just now", "Synced 3 min ago", "Synced 2 h ago" or "Synced 4 d ago".
    pub fn synced_label(&self, now: DateTime<Utc>) -> Option<String> {
        let minutes = (now - self.last?).num_minutes().max(0);
        Some(match minutes {
            0 => "Synced just now".into(),
            1..60 => format!("Synced {minutes} min ago"),
            60..1440 => format!("Synced {} h ago", minutes / 60),
            _ => format!("Synced {} d ago", minutes / 1440),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_expired_account_does_not_stop_the_other_accounts_from_syncing() {
        let accounts: Vec<_> = [
            "first@example.com",
            "expired@example.com",
            "last@example.com",
        ]
        .into_iter()
        .map(|id| Account {
            id: id.into(),
            name: id.into(),
        })
        .collect();
        let mut visited = Vec::new();
        let error = sync_accounts(&accounts, |account| {
            visited.push(account.id.clone());
            ensure!(
                account.id != "expired@example.com",
                "Reconnect this account"
            );
            Ok(())
        })
        .unwrap_err()
        .to_string();
        assert_eq!(
            visited,
            vec![
                "first@example.com",
                "expired@example.com",
                "last@example.com"
            ]
        );
        assert!(error.contains("expired@example.com: Reconnect this account"));
        assert!(!error.contains("first@example.com:"));
        assert!(!error.contains("last@example.com:"));
        assert!(sync_accounts(&accounts, |_| Ok(())).is_ok());
    }

    #[test]
    fn unreachable_google_reads_as_offline_and_a_good_sync_clears_it() {
        let accounts = [Account {
            id: "me@example.com".into(),
            name: "me@example.com".into(),
        }];
        // Nothing listens on the discard port, so the connection is refused.
        let refused = || -> Result<()> {
            reqwest::blocking::get("http://127.0.0.1:9").context("Loading calendars")?;
            Ok(())
        };
        let error = sync_accounts(&accounts, |_| refused()).unwrap_err();
        assert!(is_offline(&error));
        assert!(!is_offline(&anyhow::anyhow!("Reconnect this account")));
        let now = Utc::now();
        let mut state = SyncState {
            syncing: true,
            ..Default::default()
        };
        state.finish(&Err(error), now);
        assert!(!state.syncing && state.offline && state.error.is_some());
        assert_eq!(state.synced_label(now), None);
        state.finish(&Ok(()), now);
        assert!(!state.offline && state.error.is_none());
        let label = |minutes| state.synced_label(now + Duration::minutes(minutes));
        assert_eq!(label(0).as_deref(), Some("Synced just now"));
        assert_eq!(label(3).as_deref(), Some("Synced 3 min ago"));
        assert_eq!(label(150).as_deref(), Some("Synced 2 h ago"));
        assert_eq!(label(3000).as_deref(), Some("Synced 2 d ago"));
    }
}
