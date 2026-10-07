use chrono::{Duration, NaiveDate, Utc};
use ocal_core::{Calendar, CalendarSet, Event as CalendarEvent, Service, Snapshot, Store};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool, mpsc},
    time::Instant,
};
pub enum Command {
    Save { event: CalendarEvent, notify: bool },
    Delete(CalendarEvent),
    Respond(CalendarEvent, String),
    Sync(NaiveDate),
    Connect,
    ConnectCustom,
    Visible(Calendar),
    SaveSet(CalendarSet),
    Import,
    Export(Vec<CalendarEvent>),
}
pub enum Event {
    Snapshot(Snapshot),
    Status(String),
    Progress(String),
    Error(String),
    Saved(Box<CalendarEvent>),
    Deleted,
    Connected,
    SignInReady(String),
}
pub struct Worker {
    pub commands: mpsc::Sender<Command>,
    pub events: async_channel::Receiver<Event>,
    pub cancel_sign_in: Arc<AtomicBool>,
}
impl Worker {
    pub fn start(path: PathBuf, demo: bool) -> Self {
        let (commands, receiver) = mpsc::channel();
        let (events, output) = async_channel::unbounded();
        let cancel_sign_in = Arc::new(AtomicBool::new(false));
        let background_cancel = cancel_sign_in.clone();
        std::thread::spawn(move || {
            let init = (|| -> anyhow::Result<Service> {
                let store = if demo {
                    Store::memory()?
                } else {
                    Store::open(&path)?
                };
                let today = Utc::now()
                    .with_timezone(&ocal_core::zone(&ocal_core::system_zone()))
                    .date_naive();
                if demo {
                    ocal_core::demo::seed(&store, today)?;
                }
                Ok(Service::new(store, demo, today))
            })();
            let mut service = match init {
                Ok(s) => s,
                Err(e) => {
                    let _ = events.send_blocking(Event::Error(format!("{e:#}")));
                    return;
                }
            };
            snapshot(&service, &events);
            if !demo
                && service
                    .store
                    .snapshot()
                    .is_ok_and(|s| !s.accounts.is_empty())
            {
                match service.sync() {
                    Ok(()) => snapshot(&service, &events),
                    Err(error) => {
                        snapshot(&service, &events);
                        let _ = events.send_blocking(Event::Error(format!("{error:#}")));
                    }
                }
            }
            let mut last = Instant::now();
            let mut pending: Option<(CalendarEvent, Instant)> = None;
            loop {
                let command = match receiver.recv_timeout(std::time::Duration::from_secs(
                    if pending.is_some() { 2 } else { 30 },
                )) {
                    Ok(command) => command,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if let Some((event, started)) = pending.take()
                            && started.elapsed().as_secs() < 60
                        {
                            // Retain the cached event if polling fails; manual sync can recover it.
                            if let Ok(updated) = service.refresh(&event) {
                                if updated.conference_status == "pending" {
                                    pending = Some((updated, started));
                                }
                                snapshot(&service, &events);
                            }
                        }
                        if last.elapsed().as_secs() >= 300 && !demo {
                            match service.sync() {
                                Ok(()) => snapshot(&service, &events),
                                Err(error) => {
                                    snapshot(&service, &events);
                                    let _ =
                                        events.send_blocking(Event::Error(format!("{error:#}")));
                                }
                            }
                            last = Instant::now();
                        }
                        continue;
                    }
                };
                let result = (|| -> anyhow::Result<Option<Event>> {
                    match command {
                        Command::Save { event, notify } => {
                            let saved = service.save(&event, notify)?;
                            if saved.conference_status == "pending" {
                                pending = Some((saved.clone(), Instant::now()));
                            }
                            Ok(Some(Event::Saved(Box::new(saved))))
                        }
                        Command::Delete(e) => {
                            service.delete(&e)?;
                            Ok(Some(Event::Deleted))
                        }
                        Command::Respond(e, response) => {
                            service.respond(&e, &response)?;
                            Ok(Some(Event::Status("Invitation response saved".into())))
                        }
                        Command::Sync(date) => {
                            service.range = (date - Duration::days(60), date + Duration::days(120));
                            service.sync()?;
                            last = Instant::now();
                            Ok(Some(Event::Status(if service.demo {
                                "Demo calendar refreshed".into()
                            } else if service.store.snapshot()?.accounts.is_empty() {
                                "Local calendars are up to date".into()
                            } else {
                                "Google calendars synced".into()
                            })))
                        }
                        Command::Visible(c) => {
                            service.store.save_calendar(&c)?;
                            Ok(None)
                        }
                        Command::SaveSet(set) => {
                            service.store.save_set(&set)?;
                            Ok(Some(Event::Status("Calendar set saved".into())))
                        }
                        Command::Connect => {
                            if demo {
                                anyhow::bail!("Demo has no live accounts");
                            }
                            let config = ocal_core::oauth::ClientConfig::bundled()?;
                            let _ = events.send_blocking(Event::Progress(
                                "Preparing Google sign-in link…".into(),
                            ));
                            service.connect(
                                config,
                                |stage| {
                                    let _ = events.send_blocking(Event::Progress(stage.into()));
                                },
                                |url| {
                                    let _ = events.send_blocking(Event::SignInReady(url.into()));
                                },
                                &background_cancel,
                            )?;
                            Ok(Some(Event::Connected))
                        }
                        Command::ConnectCustom => {
                            if demo {
                                anyhow::bail!("Demo has no live accounts");
                            }
                            if let Some(file) = rfd::FileDialog::new()
                                .set_title("Google Desktop app OAuth client")
                                .add_filter("OAuth client", &["json"])
                                .pick_file()
                            {
                                let config: ocal_core::oauth::ClientFile =
                                    serde_json::from_str(&std::fs::read_to_string(file)?)?;
                                let _ = events.send_blocking(Event::Progress(
                                    "Preparing Google sign-in link…".into(),
                                ));
                                service.connect(
                                    config.installed,
                                    |stage| {
                                        let _ = events.send_blocking(Event::Progress(stage.into()));
                                    },
                                    |url| {
                                        let _ =
                                            events.send_blocking(Event::SignInReady(url.into()));
                                    },
                                    &background_cancel,
                                )?;
                                Ok(Some(Event::Connected))
                            } else {
                                Ok(Some(Event::Status("Sign-in cancelled".into())))
                            }
                        }
                        Command::Import => {
                            if let Some(file) = rfd::FileDialog::new()
                                .add_filter("iCalendar", &["ics"])
                                .pick_file()
                            {
                                let text = std::fs::read_to_string(file)?;
                                let parsed = ocal_core::ics::import(
                                    &text,
                                    "local-personal",
                                    &ocal_core::system_zone(),
                                )?;
                                let count = service.store.import(&parsed)?;
                                Ok(Some(Event::Status(format!(
                                    "Imported {count} events into Personal"
                                ))))
                            } else {
                                Ok(None)
                            }
                        }
                        Command::Export(events) => {
                            if let Some(file) = rfd::FileDialog::new()
                                .add_filter("iCalendar", &["ics"])
                                .set_file_name("ocal.ics")
                                .save_file()
                            {
                                std::fs::write(file, ocal_core::ics::export(&events))?;
                                Ok(Some(Event::Status("Calendar exported".into())))
                            } else {
                                Ok(None)
                            }
                        }
                    }
                })();
                match result {
                    Ok(event) => {
                        snapshot(&service, &events);
                        if let Some(event) = event {
                            let _ = events.send_blocking(event);
                        }
                    }
                    Err(e)
                        if e.downcast_ref::<ocal_core::oauth::SignInCancelled>()
                            .is_some() =>
                    {
                        let _ = events.send_blocking(Event::Status("Sign-in cancelled".into()));
                    }
                    Err(e) => {
                        eprintln!("Ocal: operation failed: {e:#}");
                        // A failed initial sync may still have connected the account successfully.
                        snapshot(&service, &events);
                        let _ = events.send_blocking(Event::Error(format!("{e:#}")));
                    }
                }
                if last.elapsed().as_secs() > 300 && !demo {
                    match service.sync() {
                        Ok(()) => snapshot(&service, &events),
                        Err(error) => {
                            snapshot(&service, &events);
                            let _ = events.send_blocking(Event::Error(format!("{error:#}")));
                        }
                    }
                    last = Instant::now();
                }
            }
        });
        Self {
            commands,
            events: output,
            cancel_sign_in,
        }
    }
}
fn snapshot(service: &Service, events: &async_channel::Sender<Event>) {
    match service.store.snapshot() {
        Ok(s) => {
            let _ = events.send_blocking(Event::Snapshot(s));
        }
        Err(e) => {
            let _ = events.send_blocking(Event::Error(format!("{e:#}")));
        }
    }
}
