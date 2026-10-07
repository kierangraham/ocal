//! The calendar's 15-minute cursor, shared by keyboard and pointer selection.
use chrono::{Duration, NaiveDate, NaiveDateTime};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub day: NaiveDate,
    pub minute: u16,
}
impl Slot {
    pub fn new(day: NaiveDate, minute: u16) -> Self {
        Self {
            day,
            minute: minute.min(1425) / 15 * 15,
        }
    }
    pub fn datetime(self) -> NaiveDateTime {
        self.day
            .and_hms_opt((self.minute / 60) as u32, (self.minute % 60) as u32, 0)
            .unwrap()
    }
    pub fn moved(self, days: i64, minutes: i64) -> Self {
        let d = self.datetime() + Duration::days(days) + Duration::minutes(minutes);
        Self {
            day: d.date(),
            minute: (chrono::Timelike::hour(&d) * 60 + chrono::Timelike::minute(&d)) as u16,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Selection {
    pub cursor: Slot,
    pub anchor: Option<Slot>,
    // Pointer drags select the slot under the pointer; keyboard ranges end at the line.
    inclusive_end: bool,
}
impl Selection {
    pub fn new(day: NaiveDate) -> Self {
        Self {
            cursor: Slot::new(day, 9 * 60),
            anchor: None,
            inclusive_end: false,
        }
    }
    pub fn move_by(&mut self, days: i64, minutes: i64, extend: bool) {
        if self.anchor.is_none() || !extend {
            self.inclusive_end = false;
        }
        if extend {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = self.cursor.moved(days, minutes);
    }
    pub fn begin(&mut self, slot: Slot) {
        self.cursor = slot;
        self.anchor = Some(slot);
        self.inclusive_end = true;
    }
    pub fn span(self) -> Option<(NaiveDateTime, NaiveDateTime)> {
        let a = self.anchor?.datetime();
        let b = self.cursor.datetime();
        let end = a.max(b) + Duration::minutes(if self.inclusive_end { 15 } else { 0 });
        (end > a.min(b)).then_some((a.min(b), end))
    }
    pub fn range(self) -> (NaiveDateTime, NaiveDateTime) {
        self.span().unwrap_or_else(|| {
            let start = self.cursor.datetime();
            (start, start + Duration::minutes(15))
        })
    }
    pub fn label(self) -> String {
        if self.span().is_none() {
            return self.cursor.datetime().format("%H:%M").to_string();
        }
        let (s, e) = self.range();
        if s.date() == e.date() {
            format!("{}–{}", s.format("%H:%M"), e.format("%H:%M"))
        } else {
            format!("{} → {}", s.format("%a %H:%M"), e.format("%a %H:%M"))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 5).unwrap()
    }
    #[test]
    fn shift_arrows_extend_in_quarter_hours() {
        let mut s = Selection::new(date());
        s.move_by(0, 15, true);
        s.move_by(0, 15, true);
        assert_eq!(s.label(), "09:00–09:30");
        s.move_by(0, -15, true);
        assert_eq!(s.label(), "09:00–09:15");
        s.move_by(1, 0, false);
        assert!(s.anchor.is_none());
        assert_eq!(s.cursor.day, date() + Duration::days(1));
    }
    #[test]
    fn reverse_drag_and_midnight() {
        let mut s = Selection::new(date());
        s.begin(Slot::new(date(), 615));
        s.cursor = Slot::new(date(), 571);
        assert_eq!(s.label(), "09:30–10:30");
        s.begin(Slot::new(date(), 1425));
        s.move_by(0, 15, true);
        assert_eq!((s.range().1 - s.range().0).num_minutes(), 30);
    }
    #[test]
    fn keyboard_line_extends_reverses_and_collapses_at_its_anchor() {
        let mut s = Selection::new(date());
        assert!(s.span().is_none());
        assert_eq!(s.label(), "09:00");
        s.move_by(0, -15, true);
        assert_eq!(s.label(), "08:45–09:00");
        s.move_by(0, 15, true);
        assert!(s.span().is_none());
        s.move_by(0, 15, true);
        assert_eq!(s.label(), "09:00–09:15");
        s.move_by(0, 15, false);
        assert!(s.anchor.is_none());
        assert!(s.span().is_none());
        assert_eq!((s.range().1 - s.range().0).num_minutes(), 15);
        s.cursor = Slot::new(date(), 1425);
        s.move_by(0, 15, true);
        assert_eq!((s.range().1 - s.range().0).num_minutes(), 15);
    }
}
