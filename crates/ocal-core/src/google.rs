//! Google Calendar API. Expanded instances are cached; writes use ETags.
use crate::*;
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde_json::{Value, json};
use zeroize::Zeroizing;

pub struct Google {
    client: Client,
    token: Zeroizing<String>,
}
impl Google {
    pub fn account(id: &str) -> Result<Self> {
        Ok(Self {
            client: oauth::http()?,
            token: oauth::access_token(id)?,
        })
    }
    pub fn from_tokens(tokens: &oauth::Tokens) -> Result<Self> {
        Ok(Self {
            client: oauth::http()?,
            token: Zeroizing::new(tokens.access_token().to_owned()),
        })
    }
    fn url(parts: &[&str]) -> Result<url::Url> {
        let mut u = url::Url::parse("https://www.googleapis.com/calendar/v3/")?;
        u.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("Invalid API URL"))?
            .pop_if_empty()
            .extend(parts);
        Ok(u)
    }
    fn request(&self, parts: &[&str]) -> Result<RequestBuilder> {
        Ok(self
            .client
            .get(Self::url(parts)?)
            .bearer_auth(self.token.as_str()))
    }
    pub fn calendars(&self, account: &str) -> Result<Vec<Calendar>> {
        let mut out = vec![];
        let mut page = String::new();
        loop {
            let mut req = self
                .request(&["users", "me", "calendarList"])?
                .query(&[("maxResults", "250")]);
            if !page.is_empty() {
                req = req.query(&[("pageToken", page.as_str())]);
            }
            let v = checked(req.send()?)?;
            for c in v["items"].as_array().into_iter().flatten() {
                if c["deleted"].as_bool() == Some(true) {
                    continue;
                }
                out.push(decode_calendar(c, account)?);
            }
            page = string(&v, "nextPageToken");
            if page.is_empty() {
                break;
            }
        }
        Ok(out)
    }
    pub fn primary_id(&self) -> Result<String> {
        let v = checked(
            self.request(&["users", "me", "calendarList", "primary"])?
                .send()?,
        )?;
        let id = string(&v, "id");
        ensure!(!id.is_empty(), "Google returned no primary calendar");
        Ok(id)
    }
    pub fn events(&self, c: &Calendar, from: NaiveDate, to: NaiveDate) -> Result<Vec<Event>> {
        let mut out = vec![];
        let mut page = String::new();
        let min = (from - Duration::days(2))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .to_rfc3339();
        let max = (to + Duration::days(2))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .to_rfc3339();
        loop {
            let mut req = self
                .request(&[
                    "calendars",
                    c.remote_id.as_deref().context("Missing calendar ID")?,
                    "events",
                ])?
                .query(&[
                    ("singleEvents", "true"),
                    ("orderBy", "startTime"),
                    ("maxResults", "2500"),
                    ("timeMin", min.as_str()),
                    ("timeMax", max.as_str()),
                ]);
            if !page.is_empty() {
                req = req.query(&[("pageToken", page.as_str())]);
            }
            let v = checked(req.send()?)?;
            for value in v["items"].as_array().into_iter().flatten() {
                if value["status"].as_str() == Some("cancelled") {
                    continue;
                }
                out.push(decode(value, c)?);
            }
            page = string(&v, "nextPageToken");
            if page.is_empty() {
                break;
            }
            ensure!(
                out.len() < 100_000,
                "Calendar is too large for this sync window"
            );
        }
        Ok(out)
    }
    pub fn save(&self, e: &Event, c: &Calendar, notify: bool) -> Result<Event> {
        e.validate()?;
        ensure!(!e.task, "Tasks use a local task calendar");
        let remote = c.remote_id.as_deref().context("Missing calendar ID")?;
        let mut body = payload(e);
        let url = if let Some(id) = &e.remote_id {
            Self::url(&["calendars", remote, "events", id])?
        } else {
            body["id"] = json!(e.id);
            Self::url(&["calendars", remote, "events"])?
        };
        let mut request = if e.remote_id.is_some() {
            self.client.patch(url)
        } else {
            self.client.post(url)
        }
        .bearer_auth(self.token.as_str())
        .query(&[
            ("sendUpdates", if notify { "all" } else { "none" }),
            ("conferenceDataVersion", "1"),
        ])
        .json(&body);
        if let Some(etag) = &e.etag {
            request = request.header(reqwest::header::IF_MATCH, etag);
        }
        let response = request.send()?;
        if response.status() == reqwest::StatusCode::CONFLICT && e.remote_id.is_none() {
            let existing = checked(
                self.request(&["calendars", remote, "events", &e.id])?
                    .send()?,
            )?;
            let found = decode(&existing, c)?;
            ensure!(
                found.title == e.title && found.start == e.start && found.end == e.end,
                "An event with this ID already exists with different details. Sync and review it"
            );
            return Ok(found);
        }
        decode(&checked(response)?, c)
    }
    pub fn refresh(&self, e: &Event, c: &Calendar) -> Result<Event> {
        let value = checked(
            self.request(&[
                "calendars",
                c.remote_id.as_deref().context("Missing calendar ID")?,
                "events",
                e.remote_id.as_deref().context("Missing event ID")?,
            ])?
            .send()?,
        )?;
        decode(&value, c)
    }
    pub fn delete(&self, e: &Event, c: &Calendar) -> Result<()> {
        let url = Self::url(&[
            "calendars",
            c.remote_id.as_deref().context("Missing calendar ID")?,
            "events",
            e.remote_id.as_deref().context("Missing event ID")?,
        ])?;
        let mut req = self
            .client
            .delete(url)
            .bearer_auth(self.token.as_str())
            .query(&[("sendUpdates", "none")]);
        if let Some(etag) = &e.etag {
            req = req.header(reqwest::header::IF_MATCH, etag);
        }
        let r = req.send()?;
        if matches!(r.status().as_u16(), 204 | 404 | 410) {
            return Ok(());
        }
        checked(r)?;
        Ok(())
    }
    pub fn respond(&self, e: &Event, c: &Calendar, response: &str) -> Result<Event> {
        ensure!(
            matches!(response, "accepted" | "declined" | "tentative"),
            "Invalid invitation response"
        );
        let mut attendees = e.attendees.clone();
        let mine = attendees
            .iter_mut()
            .find(|a| a.me)
            .context("This event has no invitation for you")?;
        mine.response = response.into();
        let mut req=self.client.patch(Self::url(&["calendars",c.remote_id.as_deref().context("Missing calendar ID")?,"events",e.remote_id.as_deref().context("Missing event ID")?])?).bearer_auth(self.token.as_str()).query(&[("sendUpdates","all")]).json(&json!({"attendees":attendees.iter().map(|a|json!({"email":a.email,"responseStatus":a.response})).collect::<Vec<_>>() }));
        if let Some(etag) = &e.etag {
            req = req.header(reqwest::header::IF_MATCH, etag);
        }
        decode(&checked(req.send()?)?, c)
    }
}
fn decode_calendar(c: &Value, account: &str) -> Result<Calendar> {
    let remote = string(c, "id");
    ensure!(
        !remote.is_empty(),
        "Google returned a calendar without an ID"
    );
    let role = string(c, "accessRole");
    Ok(Calendar {
        id: format!("google:{account}:{remote}"),
        account: Some(account.into()),
        remote_id: Some(remote.clone()),
        name: c["summaryOverride"]
            .as_str()
            .or(c["summary"].as_str())
            .unwrap_or(&remote)
            .into(),
        color: c["backgroundColor"].as_str().unwrap_or("#7aa2f7").into(),
        timezone: c["timeZone"].as_str().unwrap_or("UTC").into(),
        writable: matches!(role.as_str(), "owner" | "writer"),
        visible: c["selected"].as_bool().unwrap_or(true),
        meet_available: c["conferenceProperties"]["allowedConferenceSolutionTypes"]
            .as_array()
            .is_some_and(|v| v.iter().any(|s| s.as_str() == Some("hangoutsMeet"))),
    })
}

fn checked(r: Response) -> Result<Value> {
    let status = r.status();
    if !status.is_success() {
        // Keep provider diagnostics, but never render an arbitrary response body or token.
        let error: Value =
            serde_json::from_reader(std::io::Read::take(r, 64 * 1024)).unwrap_or(Value::Null);
        return Err(api_error(status, &error));
    }
    if status == reqwest::StatusCode::NO_CONTENT {
        return Ok(Value::Null);
    }
    Ok(r.json()?)
}

fn api_error(status: reqwest::StatusCode, body: &Value) -> anyhow::Error {
    let reasons: Vec<&str> = ["errors", "details"]
        .into_iter()
        .flat_map(|key| body["error"][key].as_array().into_iter().flatten())
        .filter_map(|detail| detail["reason"].as_str())
        .filter(|reason| {
            !reason.is_empty()
                && reason.len() <= 64
                && reason
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
        .take(8)
        .collect();
    let has = |values: &[&str]| reasons.iter().any(|reason| values.contains(reason));
    if status == reqwest::StatusCode::PRECONDITION_FAILED {
        return anyhow::anyhow!("This event changed on Google. Sync before editing it again");
    }
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return anyhow::anyhow!("Google sign-in expired. Reconnect this account");
    }
    if status == reqwest::StatusCode::FORBIDDEN && has(&["accessNotConfigured", "SERVICE_DISABLED"])
    {
        return anyhow::anyhow!(
            "Google Calendar API is disabled for this app. Enable Google Calendar API in its Google Cloud project, then try signing in again"
        );
    }
    if status == reqwest::StatusCode::FORBIDDEN
        && (has(&["insufficientPermissions", "ACCESS_TOKEN_SCOPE_INSUFFICIENT"])
            || body["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("insufficient authentication scopes")))
    {
        return anyhow::anyhow!(
            "Google did not grant Calendar access. Sign in again and approve all requested calendar permissions"
        );
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || has(&[
            "rateLimitExceeded",
            "userRateLimitExceeded",
            "quotaExceeded",
            "RATE_LIMIT_EXCEEDED",
        ])
    {
        return anyhow::anyhow!(
            "Google Calendar’s request limit was reached. Try syncing again shortly"
        );
    }
    let reason = if reasons.is_empty() {
        String::new()
    } else {
        format!("; {}", reasons.join(", "))
    };
    anyhow::anyhow!(
        "Google Calendar request failed ({status}{reason}). Your cached calendar is preserved"
    )
}
fn string(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or("").into()
}
fn time(v: &Value, tz: &str) -> Result<(DateTime<Utc>, Option<NaiveDate>)> {
    if let Some(s) = v["date"].as_str() {
        let d = NaiveDate::parse_from_str(s, "%Y-%m-%d")?;
        return Ok((d.and_hms_opt(0, 0, 0).unwrap().and_utc(), Some(d)));
    }
    let s = v["dateTime"]
        .as_str()
        .context("Event has no start/end time")?;
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Ok((d.with_timezone(&Utc), None));
    }
    Ok((
        local_datetime(
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")?,
            tz,
        )?,
        None,
    ))
}
pub fn decode(v: &Value, c: &Calendar) -> Result<Event> {
    let tz = v["start"]["timeZone"].as_str().unwrap_or(&c.timezone);
    let (start, s) = time(&v["start"], tz)?;
    let (end, t) = time(&v["end"], tz)?;
    let id = string(v, "id");
    ensure!(!id.is_empty(), "Event has no ID");
    let mut e = Event::new(&c.id, start.date_naive(), tz)?;
    e.id = format!("{}:{id}", c.id);
    e.remote_id = Some(id);
    e.etag = v["etag"].as_str().map(str::to_owned);
    e.series_id = v["recurringEventId"].as_str().map(str::to_owned);
    e.title = v["summary"].as_str().unwrap_or("Untitled event").into();
    e.start = start;
    e.end = end;
    e.timezone = tz.into();
    e.all_day = s.zip(t);
    e.location = string(v, "location");
    e.notes = string(v, "description");
    e.link = string(v, "hangoutLink");
    if e.link.is_empty() {
        e.link = v["conferenceData"]["entryPoints"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["entryPointType"].as_str() == Some("video"))
            .and_then(|p| p["uri"].as_str())
            .unwrap_or("")
            .into();
    }
    e.has_conference = v["conferenceData"].is_object();
    e.conference_status = v["conferenceData"]["createRequest"]["status"]["statusCode"]
        .as_str()
        .unwrap_or("")
        .into();
    if e.link.is_empty() {
        e.link = e
            .notes
            .split_whitespace()
            .find(|s| safe_url(s).is_ok())
            .unwrap_or("")
            .into();
    }
    e.busy = v["transparency"].as_str() != Some("transparent");
    e.attendees = v["attendees"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| {
            a["email"].as_str().map(|email| Attendee {
                email: email.into(),
                response: a["responseStatus"].as_str().unwrap_or("needsAction").into(),
                me: a["self"].as_bool().unwrap_or(false),
            })
        })
        .collect();
    e.reminder = v["reminders"]["overrides"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["method"].as_str() == Some("popup"))
        .and_then(|r| r["minutes"].as_u64())
        .map(|n| n as u32);
    Ok(e)
}
pub fn payload(e: &Event) -> Value {
    let (start, end) = if let Some((s, t)) = e.all_day {
        (json!({"date":s.to_string()}), json!({"date":t.to_string()}))
    } else {
        (
            json!({"dateTime":e.start.to_rfc3339(),"timeZone":e.timezone}),
            json!({"dateTime":e.end.to_rfc3339(),"timeZone":e.timezone}),
        )
    };
    let mut v = json!({"summary":e.title,"start":start,"end":end,"location":e.location,"description":if e.link.is_empty()||e.notes.contains(&e.link){e.notes.clone()}else{format!("{}\n\n{}",e.notes,e.link).trim().to_owned()},"transparency":if e.busy{"opaque"}else{"transparent"},"reminders":{"useDefault":false,"overrides":e.reminder.map(|n|vec![json!({"method":"popup","minutes":n})]).unwrap_or_default()},"attendees":e.attendees.iter().map(|a|json!({"email":a.email,"responseStatus":a.response})).collect::<Vec<_>>()});
    if e.clear_conference {
        v["conferenceData"] = Value::Null;
    }
    if let Some(request) = &e.meet_request {
        v["conferenceData"] = json!({"createRequest": {
            "requestId": request,
            "conferenceSolutionKey": {"type": "hangoutsMeet"}
        }});
    }
    if e.series_id.is_none() {
        let rule = e.repeat.rrule().map(|mut rule| {
            if let Some(count) = e.repeat_count {
                rule.push_str(&format!(";COUNT={count}"));
            } else if let Some(until) = e.repeat_until {
                let end = if e.all_day.is_some() {
                    until.format("%Y%m%d").to_string()
                } else {
                    crate::local_datetime(until.and_hms_opt(23, 59, 59).unwrap(), &e.timezone)
                        .expect("validated time zone")
                        .format("%Y%m%dT%H%M%SZ")
                        .to_string()
                };
                rule.push_str(&format!(";UNTIL={end}"));
            }
            rule
        });
        v["recurrence"] = json!(rule.into_iter().collect::<Vec<_>>());
    }
    v
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_calendars_and_events_are_namespaced_by_the_signed_in_account() {
        let shared = json!({"id":"team@example.com", "summary":"Work", "accessRole":"writer"});
        let first = decode_calendar(&shared, "first@example.com").unwrap();
        let second = decode_calendar(&shared, "second@example.com").unwrap();
        assert_eq!(first.remote_id, second.remote_id);
        assert_ne!(first.id, second.id);
        assert_eq!(first.account.as_deref(), Some("first@example.com"));
        assert_eq!(second.account.as_deref(), Some("second@example.com"));
        assert_ne!(first.display_name(), second.display_name());
        let event = json!({"id":"same-event", "summary":"Shared review",
            "start":{"dateTime":"2026-10-05T09:00:00Z"},
            "end":{"dateTime":"2026-10-05T10:00:00Z"}});
        let first_event = decode(&event, &first).unwrap();
        let second_event = decode(&event, &second).unwrap();
        assert_ne!(first_event.id, second_event.id);
        assert_eq!(first_event.calendar, first.id);
        assert_eq!(second_event.calendar, second.id);
    }

    fn response(status: &str, body: &str) -> Response {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let reply = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).unwrap() > 0);
            stream.write_all(reply.as_bytes()).unwrap();
        });
        let response = Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("http://{address}"))
            .send()
            .unwrap();
        server.join().unwrap();
        response
    }

    #[test]
    fn calendar_errors_distinguish_disabled_api_from_missing_permissions_and_quota() {
        for body in [
            r#"{"error":{"errors":[{"reason":"accessNotConfigured"}]}}"#,
            r#"{"error":{"details":[{"reason":"SERVICE_DISABLED"}]}}"#,
        ] {
            let error = checked(response("403 Forbidden", body))
                .unwrap_err()
                .to_string();
            assert!(error.contains("API is disabled"));
            assert!(error.contains("Google Cloud project"));
        }
        let error = checked(response(
            "403 Forbidden",
            r#"{"error":{"details":[{"reason":"ACCESS_TOKEN_SCOPE_INSUFFICIENT"}]}}"#,
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains("approve all requested calendar permissions"));
        let error = checked(response(
            "403 Forbidden",
            r#"{"error":{"errors":[{"reason":"rateLimitExceeded"}]}}"#,
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains("request limit"));
    }

    #[test]
    fn calendar_errors_preserve_safe_reasons_without_dumping_response_data() {
        let error = checked(response(
            "403 Forbidden",
            r#"{"error":{"errors":[{"reason":"forbidden"}],"message":"private-provider-data"}}"#,
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains("forbidden"));
        assert!(!error.contains("private-provider-data"));
        let error = checked(response(
            "502 Bad Gateway",
            "<html>private-provider-data</html>",
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains("502"));
        assert!(!error.contains("private-provider-data"));
        assert!(checked(response("204 No Content", "")).unwrap().is_null());
        assert_eq!(
            checked(response("200 OK", r#"{"items":[]}"#)).unwrap(),
            json!({"items":[]})
        );
        assert!(
            checked(response("412 Precondition Failed", ""))
                .unwrap_err()
                .to_string()
                .contains("changed on Google")
        );
    }

    #[test]
    fn meet_generation_is_explicit_and_preserves_the_request_id_on_retry() {
        let mut event = Event::new("g", Utc::now().date_naive(), "UTC").unwrap();
        event.title = "Review".into();
        assert!(payload(&event).get("conferenceData").is_none());
        event.meet_request = Some("stable-request".into());
        let body = payload(&event);
        assert_eq!(
            body["conferenceData"]["createRequest"]["requestId"],
            "stable-request"
        );
        assert_eq!(
            body["conferenceData"]["createRequest"]["conferenceSolutionKey"]["type"],
            "hangoutsMeet"
        );
        assert_eq!(payload(&event)["conferenceData"], body["conferenceData"]);
    }
    #[test]
    fn explicit_removal_and_repeat_limits_survive_patch() {
        let mut e = Event::new("g", Utc::now().date_naive(), "Europe/London").unwrap();
        e.title = "Review".into();
        e.clear_conference = true;
        e.repeat = crate::Repeat::Weekly;
        e.repeat_count = Some(4);
        let body = payload(&e);
        assert_eq!(body.get("conferenceData"), Some(&Value::Null));
        assert_eq!(body["recurrence"][0], "RRULE:FREQ=WEEKLY;COUNT=4");
    }
    #[test]
    fn date_only_is_exclusive_and_timezone_independent() {
        let c = Calendar {
            id: "g".into(),
            account: Some("a".into()),
            remote_id: Some("x".into()),
            name: "Work".into(),
            color: "#ffffff".into(),
            timezone: "Pacific/Auckland".into(),
            writable: true,
            visible: true,
            meet_available: false,
        };
        let v = json!({"id":"opaque_1","summary":"Holiday","start":{"date":"2026-10-05"},"end":{"date":"2026-10-07"},"etag":"a"});
        let e = decode(&v, &c).unwrap();
        assert!(e.on_date(
            NaiveDate::from_ymd_opt(2026, 10, 6).unwrap(),
            chrono_tz::America::Los_Angeles
        ));
        assert!(!e.on_date(
            NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(),
            chrono_tz::UTC
        ));
        assert_eq!(payload(&e)["end"]["date"], "2026-10-07");
    }
    #[test]
    fn instance_patch_preserves_recurrence() {
        let mut e = Event::new("g", Utc::now().date_naive(), "UTC").unwrap();
        e.title = "Review".into();
        e.series_id = Some("series".into());
        assert!(payload(&e).get("recurrence").is_none());
    }
}
