use crate::*;
use anyhow::{Context, Result, ensure};
use chrono::{Duration, NaiveDate};
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
    for account in accounts {
        if let Err(error) = sync(account) {
            failures.push(format!("{}: {error:#}", account.id));
        }
    }
    ensure!(
        failures.is_empty(),
        "Some Google accounts could not sync:\n{}",
        failures.join("\n")
    );
    Ok(())
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
}
