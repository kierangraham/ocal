//! Small standards-based import/export for single events and simple recurrences.
use crate::*;
use anyhow::{Context, Result, ensure};
use chrono::{Duration, NaiveDate, NaiveDateTime};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub fn export(events: &[Event]) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".into(),
        "VERSION:2.0".into(),
        "PRODID:-//Ocal//Calendar for Omarchy//EN".into(),
        "CALSCALE:GREGORIAN".into(),
    ];
    for e in events {
        lines.push(
            if e.task {
                "BEGIN:VTODO"
            } else {
                "BEGIN:VEVENT"
            }
            .into(),
        );
        lines.push(format!("UID:{}@ocal", e.id));
        lines.push(format!(
            "DTSTAMP:{}",
            chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
        ));
        lines.push(format!("SUMMARY:{}", escape(&e.title)));
        if let Some((s, t)) = e.all_day {
            lines.push(format!("DTSTART;VALUE=DATE:{}", s.format("%Y%m%d")));
            lines.push(format!("DTEND;VALUE=DATE:{}", t.format("%Y%m%d")));
        } else if e.repeat != Repeat::Never {
            let tz = zone(&e.timezone);
            lines.push(format!(
                "DTSTART;TZID={}:{}",
                e.timezone,
                e.start.with_timezone(&tz).format("%Y%m%dT%H%M%S")
            ));
            lines.push(format!(
                "DTEND;TZID={}:{}",
                e.timezone,
                e.end.with_timezone(&tz).format("%Y%m%dT%H%M%S")
            ));
        } else {
            lines.push(format!("DTSTART:{}", e.start.format("%Y%m%dT%H%M%SZ")));
            lines.push(format!("DTEND:{}", e.end.format("%Y%m%dT%H%M%SZ")));
        }
        if !e.location.is_empty() {
            lines.push(format!("LOCATION:{}", escape(&e.location)));
        }
        if !e.notes.is_empty() {
            lines.push(format!("DESCRIPTION:{}", escape(&e.notes)));
        }
        if !e.link.is_empty() {
            lines.push(format!("URL:{}", e.link));
        }
        if let Some(mut rule) = e.repeat.rrule() {
            if let Some(count) = e.repeat_count {
                rule.push_str(&format!(";COUNT={count}"));
            }
            if let Some(until) = e.repeat_until {
                rule.push_str(&format!(";UNTIL={}T235959Z", until.format("%Y%m%d")));
            }
            lines.push(rule);
        }
        if e.task {
            lines.push(format!(
                "STATUS:{}",
                if e.completed {
                    "COMPLETED"
                } else {
                    "NEEDS-ACTION"
                }
            ));
        } else {
            lines.push(format!(
                "TRANSP:{}",
                if e.busy { "OPAQUE" } else { "TRANSPARENT" }
            ));
        }
        lines.push(if e.task { "END:VTODO" } else { "END:VEVENT" }.into());
    }
    lines.push("END:VCALENDAR".into());
    lines
        .iter()
        .map(|s| fold(s))
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n"
}
pub fn import(text: &str, calendar: &str, timezone: &str) -> Result<Vec<Event>> {
    ensure!(text.len() <= 10 * 1024 * 1024, "Calendar file is too large");
    let normalized = text.replace("\r\n", "\n");
    let mut lines: Vec<String> = vec![];
    for l in normalized.lines() {
        if l.starts_with([' ', '\t']) {
            if let Some(last) = lines.last_mut() {
                last.push_str(&l[1..]);
            }
        } else {
            lines.push(l.to_owned());
        }
    }
    ensure!(
        lines.first().is_some_and(|s| s == "BEGIN:VCALENDAR"),
        "Choose an iCalendar (.ics) file"
    );
    let mut fields = BTreeMap::new();
    let mut active = false;
    let mut task = false;
    let mut out = vec![];
    for l in lines {
        if l == "BEGIN:VEVENT" || l == "BEGIN:VTODO" {
            fields.clear();
            active = true;
            task = l == "BEGIN:VTODO";
        } else if l == "END:VEVENT" || l == "END:VTODO" {
            if active {
                out.push(parse(&fields, calendar, timezone, task)?);
                ensure!(out.len() < 10000, "Calendar has too many events");
            }
            active = false;
        } else if active && let Some((key, value)) = l.split_once(':') {
            fields.insert(key.to_owned(), value.to_owned());
        }
    }
    Ok(out)
}
fn parse(f: &BTreeMap<String, String>, calendar: &str, tz: &str, task: bool) -> Result<Event> {
    let start = f
        .iter()
        .find(|(k, _)| k.split(';').next() == Some("DTSTART"))
        .context("An imported event has no DTSTART")?;
    let (s, date, timezone) = datetime(start.0, start.1, tz)?;
    let end = f.iter().find(|(k, _)| k.split(';').next() == Some("DTEND"));
    let (t, end_date, _) = if let Some((k, v)) = end {
        datetime(k, v, &timezone)?
    } else {
        (
            s + if date.is_some() {
                Duration::days(1)
            } else {
                Duration::hours(1)
            },
            date.map(|d| d + Duration::days(1)),
            timezone.clone(),
        )
    };
    let mut e = Event::new(calendar, s.date_naive(), &timezone)?;
    let uid = f.get("UID").context("An imported event has no UID")?;
    e.id = format!("{:x}", Sha256::digest(format!("{calendar}:{uid}")))
        .chars()
        .take(32)
        .collect();
    e.start = s;
    e.end = t;
    e.all_day = date.zip(end_date);
    e.timezone = timezone;
    e.title = unescape(
        f.get("SUMMARY")
            .map(String::as_str)
            .unwrap_or("Untitled event"),
    );
    e.location = unescape(f.get("LOCATION").map(String::as_str).unwrap_or(""));
    e.notes = unescape(f.get("DESCRIPTION").map(String::as_str).unwrap_or(""));
    e.link = f.get("URL").cloned().unwrap_or_default();
    e.task = task;
    e.completed = f.get("STATUS").is_some_and(|s| s == "COMPLETED");
    e.busy = f.get("TRANSP").is_none_or(|s| s != "TRANSPARENT");
    ensure!(
        !f.keys().any(|k| matches!(
            k.split(';').next(),
            Some("RECURRENCE-ID" | "EXDATE" | "RDATE")
        )),
        "This file has recurrence exceptions. Connect its Google calendar to preserve them"
    );
    if let Some(rule) = f.get("RRULE") {
        for pair in rule.split(';') {
            let (k, v) = pair.split_once('=').context("Invalid recurrence")?;
            match k {
                "FREQ" => {
                    e.repeat = match v {
                        "DAILY" => Repeat::Daily,
                        "WEEKLY" => Repeat::Weekly,
                        "MONTHLY" => Repeat::Monthly,
                        "YEARLY" => Repeat::Yearly,
                        _ => anyhow::bail!("Unsupported recurrence frequency"),
                    }
                }
                "COUNT" => e.repeat_count = Some(v.parse()?),
                "UNTIL" => {
                    e.repeat_until = Some(NaiveDate::parse_from_str(
                        v.get(..8).context("Invalid recurrence end")?,
                        "%Y%m%d",
                    )?)
                }
                "INTERVAL" if v == "1" => (),
                _ => anyhow::bail!(
                    "This file has a complex recurrence. Connect its Google calendar to preserve it"
                ),
            }
        }
    }
    e.validate()?;
    Ok(e)
}
fn datetime(
    key: &str,
    value: &str,
    fallback: &str,
) -> Result<(chrono::DateTime<chrono::Utc>, Option<NaiveDate>, String)> {
    let tz = key
        .split(';')
        .find_map(|s| s.strip_prefix("TZID="))
        .unwrap_or(fallback);
    if value.len() == 8 {
        let date = NaiveDate::parse_from_str(value, "%Y%m%d")?;
        return Ok((
            date.and_hms_opt(0, 0, 0).unwrap().and_utc(),
            Some(date),
            tz.into(),
        ));
    }
    let d = NaiveDateTime::parse_from_str(value.trim_end_matches('Z'), "%Y%m%dT%H%M%S")?;
    Ok((
        if value.ends_with('Z') {
            d.and_utc()
        } else {
            local_datetime(d, tz)?
        },
        None,
        tz.into(),
    ))
}
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\r', "")
}
fn unescape(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(c) = chars.next() {
                result.push(if matches!(c, 'n' | 'N') { '\n' } else { c })
            }
        } else {
            result.push(c)
        }
    }
    result
}
fn fold(s: &str) -> String {
    let mut out = String::new();
    let mut count = 0;
    for c in s.chars() {
        if count + c.len_utf8() > 75 {
            out.push_str("\r\n ");
            count = 1;
        }
        out.push(c);
        count += c.len_utf8();
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folded_unicode_and_all_day_round_trip() {
        let mut e = Event::new(
            "local-personal",
            NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
            "Europe/London",
        )
        .unwrap();
        e.title = "Café, design; notes ".repeat(12);
        e.notes = "First line\nSecond line \\".into();
        e.all_day = Some((e.date(), e.date() + Duration::days(2)));
        let text = export(&[e.clone()]);
        assert!(text.lines().all(|s| s.len() <= 75));
        let back = import(&text, "local-personal", "Europe/London").unwrap();
        assert_eq!(back[0].title, e.title);
        assert_eq!(back[0].notes, e.notes);
        assert_eq!(back[0].all_day, e.all_day);
    }
    #[test]
    fn unsupported_rules_are_visible_errors() {
        let text = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:a\nSUMMARY:a\nDTSTART:20261005T090000Z\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE\nEND:VEVENT\nEND:VCALENDAR";
        assert!(import(text, "local-personal", "UTC").is_err());
    }
}
