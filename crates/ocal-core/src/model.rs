use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Calendar {
    pub id: String,
    pub account: Option<String>,
    pub remote_id: Option<String>,
    pub name: String,
    pub color: String,
    pub timezone: String,
    pub writable: bool,
    pub visible: bool,
    #[serde(default)]
    pub meet_available: bool,
}
impl Calendar {
    pub fn display_name(&self) -> String {
        match &self.account {
            Some(account) => format!("{} · {account}", self.name),
            None => format!("{} · Local", self.name),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CalendarSet {
    pub id: String,
    pub name: String,
    pub calendars: Vec<String>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeat {
    #[default]
    Never,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}
impl Repeat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Never => "Does not repeat",
            Self::Daily => "Every day",
            Self::Weekly => "Every week",
            Self::Monthly => "Every month",
            Self::Yearly => "Every year",
        }
    }
    pub fn rrule(self) -> Option<String> {
        match self {
            Self::Never => None,
            Self::Daily => Some("RRULE:FREQ=DAILY".into()),
            Self::Weekly => Some("RRULE:FREQ=WEEKLY".into()),
            Self::Monthly => Some("RRULE:FREQ=MONTHLY".into()),
            Self::Yearly => Some("RRULE:FREQ=YEARLY".into()),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub calendar: String,
    pub remote_id: Option<String>,
    pub etag: Option<String>,
    pub series_id: Option<String>,
    pub title: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub timezone: String,
    /// Date-only ranges use an exclusive end and never convert with the display zone.
    pub all_day: Option<(NaiveDate, NaiveDate)>,
    pub location: String,
    pub notes: String,
    pub link: String,
    #[serde(default)]
    pub meet_request: Option<String>,
    #[serde(default)]
    pub conference_status: String,
    #[serde(default)]
    pub has_conference: bool,
    /// Explicitly remove Google conference data when editing.
    #[serde(default)]
    pub clear_conference: bool,
    pub attendees: Vec<Attendee>,
    pub repeat: Repeat,
    pub repeat_until: Option<NaiveDate>,
    pub repeat_count: Option<u32>,
    pub reminder: Option<u32>,
    pub busy: bool,
    pub task: bool,
    pub completed: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attendee {
    pub email: String,
    pub response: String,
    pub me: bool,
}
impl Event {
    pub fn new(calendar: &str, date: NaiveDate, timezone: &str) -> Result<Self> {
        let start = local_datetime(date.and_hms_opt(9, 0, 0).context("Invalid date")?, timezone)?;
        Ok(Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            calendar: calendar.into(),
            remote_id: None,
            etag: None,
            series_id: None,
            title: String::new(),
            start,
            end: start + Duration::hours(1),
            timezone: timezone.into(),
            all_day: None,
            location: String::new(),
            notes: String::new(),
            link: String::new(),
            meet_request: None,
            conference_status: String::new(),
            has_conference: false,
            clear_conference: false,
            attendees: vec![],
            repeat: Repeat::Never,
            repeat_until: None,
            repeat_count: None,
            reminder: Some(15),
            busy: true,
            task: false,
            completed: false,
        })
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.title.trim().is_empty(), "Give your event a title");
        ensure!(self.end > self.start, "The end must be after the start");
        self.timezone
            .parse::<Tz>()
            .context("Use an IANA time zone, such as Europe/London")?;
        if let Some((start, end)) = self.all_day {
            ensure!(end > start, "An all-day event needs at least one day");
        }
        ensure!(
            self.repeat_count != Some(0),
            "Repeat count must be at least one"
        );
        if let Some(until) = self.repeat_until {
            ensure!(until >= self.date(), "Repeat end cannot precede the event");
        }
        ensure!(
            self.attendees.iter().all(|a| valid_email(&a.email)),
            "Check the guest email addresses"
        );
        if !self.link.is_empty() {
            safe_url(&self.link)?;
        }
        Ok(())
    }
    pub fn date(&self) -> NaiveDate {
        self.all_day.map_or_else(
            || self.start.with_timezone(&zone(&self.timezone)).date_naive(),
            |(s, _)| s,
        )
    }
    pub fn dates_in(&self, tz: Tz) -> (NaiveDate, NaiveDate) {
        self.all_day.unwrap_or_else(|| {
            (
                self.start.with_timezone(&tz).date_naive(),
                (self.end - Duration::seconds(1))
                    .with_timezone(&tz)
                    .date_naive()
                    + Duration::days(1),
            )
        })
    }
    pub fn on_date(&self, date: NaiveDate, tz: Tz) -> bool {
        let (s, e) = self.dates_in(tz);
        date >= s && date < e
    }
    pub fn time_label(&self, tz: Tz) -> String {
        if self.all_day.is_some() {
            "All day".into()
        } else {
            format!(
                "{}–{}",
                self.start.with_timezone(&tz).format("%H:%M"),
                self.end.with_timezone(&tz).format("%H:%M")
            )
        }
    }
}
pub fn valid_email(s: &str) -> bool {
    !s.contains(['\r', '\n', ' ', ',', ';'])
        && s.split_once('@')
            .is_some_and(|(a, b)| !a.is_empty() && b.contains('.') && !b.starts_with('.'))
}
pub fn safe_url(s: &str) -> Result<url::Url> {
    let u = url::Url::parse(s)?;
    ensure!(
        matches!(u.scheme(), "http" | "https") && u.host_str().is_some(),
        "Use an http or https link"
    );
    Ok(u)
}
pub fn zone(s: &str) -> Tz {
    s.parse().unwrap_or(chrono_tz::UTC)
}
/// The desktop's IANA zone from `TZ` or `/etc/localtime`, falling back to UTC.
pub fn system_zone() -> String {
    let valid = |tz: &String| tz.parse::<Tz>().is_ok();
    std::env::var("TZ")
        .ok()
        .map(|tz| tz.trim_start_matches(':').to_owned())
        .filter(valid)
        .or_else(|| iana_time_zone::get_timezone().ok().filter(valid))
        .unwrap_or_else(|| "UTC".into())
}
pub fn local_datetime(dt: NaiveDateTime, tz: &str) -> Result<DateTime<Utc>> {
    let tz: Tz = tz.parse().context("Unknown time zone")?;
    tz.from_local_datetime(&dt).single().map(|v|v.with_timezone(&Utc)).context("This time is ambiguous or does not exist because the clocks change. Choose another time or use UTC")
}
pub fn month_start(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap()
}
pub fn move_month(d: NaiveDate, n: i32) -> NaiveDate {
    let m = d.year() * 12 + d.month0() as i32 + n;
    let first = NaiveDate::from_ymd_opt(m.div_euclid(12), m.rem_euclid(12) as u32 + 1, 1).unwrap();
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
    }
    .unwrap();
    first
        .with_day(d.day().min((next - Duration::days(1)).day()))
        .unwrap()
}
/// Weeks start on Sunday, matching the date picker.
pub fn week_start(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_sunday() as i64)
}
pub fn month_days(d: NaiveDate) -> Vec<NaiveDate> {
    let start = week_start(month_start(d));
    (0..42).map(|i| start + Duration::days(i)).collect()
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub calendars: Vec<Calendar>,
    pub accounts: Vec<Account>,
    pub events: Vec<Event>,
    pub sets: Vec<CalendarSet>,
}
