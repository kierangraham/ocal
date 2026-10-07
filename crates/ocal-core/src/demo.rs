use crate::*;
use anyhow::Result;
use chrono::{Datelike, Duration, NaiveDate};
pub fn seed(store: &Store, today: NaiveDate) -> Result<()> {
    let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);
    for (day, hour, minutes, duration, title, cal, location) in [
        (0, 9, 0, 30, "Weekly planning", "local-work", "Studio"),
        (0, 10, 0, 90, "Deep work", "local-work", ""),
        (0, 10, 30, 30, "Design review", "local-personal", ""),
        (
            0,
            13,
            0,
            45,
            "Coffee with Oliver",
            "local-personal",
            "Established",
        ),
        (1, 9, 30, 30, "Team stand-up", "local-work", ""),
        (1, 11, 0, 60, "Product review", "local-work", "Studio"),
        (
            1,
            15,
            0,
            60,
            "Long walk",
            "local-personal",
            "Botanic Gardens",
        ),
        (2, 10, 0, 120, "Build time", "local-work", ""),
        (2, 13, 0, 60, "Lunch with friends", "local-personal", ""),
        (2, 16, 0, 30, "Catch-up", "local-work", ""),
        (3, 9, 0, 60, "Project kickoff", "local-work", ""),
        (3, 11, 30, 45, "Dentist", "local-personal", ""),
        (3, 14, 0, 90, "Focus time", "local-work", ""),
        (4, 9, 30, 30, "Weekly review", "local-work", ""),
        (4, 12, 0, 60, "Friday lunch", "local-personal", ""),
        (4, 15, 0, 30, "Wrap up", "local-work", ""),
        (
            5,
            10,
            0,
            120,
            "Coastal walk",
            "local-personal",
            "North coast",
        ),
        (6, 10, 0, 60, "Slow Sunday", "local-personal", ""),
        (6, 14, 0, 90, "Read and reset", "local-personal", ""),
    ] {
        let date = monday + Duration::days(day);
        let mut e = Event::new(cal, date, "Europe/London")?;
        e.title = title.into();
        e.start = local_datetime(date.and_hms_opt(hour, minutes, 0).unwrap(), "Europe/London")?;
        e.end = e.start + Duration::minutes(duration);
        e.location = location.into();
        if title == "Team stand-up" {
            e.link = "https://meet.google.com/abc-defg-hij".into();
            e.notes = "A short check-in: progress, plans, and anything in the way.".into();
        }
        store.save_event(&e)?;
    }
    let mut task = Event::new("local-tasks", today, "Europe/London")?;
    task.title = "Review the design notes".into();
    task.task = true;
    task.busy = false;
    store.save_event(&task)?;
    let mut all = Event::new(
        "local-personal",
        monday + Duration::days(2),
        "Europe/London",
    )?;
    all.title = "A little space to think".into();
    all.all_day = Some((monday + Duration::days(2), monday + Duration::days(3)));
    store.save_event(&all)?;
    store.save_set(&CalendarSet {
        id: "work".into(),
        name: "Work".into(),
        calendars: vec!["local-work".into()],
    })?;
    store.save_set(&CalendarSet {
        id: "personal".into(),
        name: "Personal".into(),
        calendars: vec!["local-personal".into(), "local-tasks".into()],
    })?;
    Ok(())
}
