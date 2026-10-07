//! Date expansion and overlap layout independent of the renderer.
use crate::{Event, Repeat, local_datetime, zone};
use chrono::{Datelike, Duration, NaiveDate};

pub fn occurrences(event: &Event, from: NaiveDate, to: NaiveDate) -> Vec<Event> {
    if event.repeat == Repeat::Never {
        let (s, e) = event.dates_in(zone(&event.timezone));
        return if e > from && s < to {
            vec![event.clone()]
        } else {
            vec![]
        };
    }
    let tz = zone(&event.timezone);
    let original = event.start.with_timezone(&tz).naive_local();
    let date = event.date();
    let mut out = vec![];
    // Without a count limit, skip whole periods that end before `from` rather than
    // expanding the series from its first occurrence on every render.
    let first = if event.repeat_count.is_none() {
        let (s, e) = event.dates_in(tz);
        let lead = (from - date).num_days() - (e - s).num_days() - 2;
        let months = || {
            let m = (from.year() - date.year()) * 12 + from.month0() as i32 - date.month0() as i32;
            (m - (e - s).num_days() as i32 / 28 - 2).max(0) as u32
        };
        match event.repeat {
            _ if lead <= 0 => 0,
            Repeat::Daily => lead as u32,
            Repeat::Weekly => (lead / 7) as u32,
            Repeat::Monthly => months(),
            Repeat::Yearly => months() / 12,
            Repeat::Never => unreachable!(),
        }
    } else {
        0
    };
    // Bound the recurrence scan, retaining calendar dates across daylight saving.
    let mut count = 0;
    for i in first..first.saturating_add(100_000) {
        if event.repeat_count.is_some_and(|n| count >= n) {
            break;
        }
        let next = match event.repeat {
            Repeat::Daily => date.checked_add_signed(Duration::days(i as i64)),
            Repeat::Weekly => date.checked_add_signed(Duration::weeks(i as i64)),
            Repeat::Monthly => {
                let m = date.year() * 12 + date.month0() as i32 + i as i32;
                NaiveDate::from_ymd_opt(m.div_euclid(12), m.rem_euclid(12) as u32 + 1, date.day())
            }
            Repeat::Yearly => {
                NaiveDate::from_ymd_opt(date.year() + i as i32, date.month(), date.day())
            }
            Repeat::Never => unreachable!(),
        };
        let Some(next) = next else { continue };
        if next >= to || event.repeat_until.is_some_and(|until| next > until) {
            break;
        }
        let (start, end, all_day) = if let Some((s, e)) = event.all_day {
            let start = next.and_hms_opt(0, 0, 0).unwrap().and_utc();
            (start, start + (e - s), Some((next, next + (e - s))))
        } else {
            let Ok(start) = local_datetime(next.and_time(original.time()), &event.timezone) else {
                continue;
            };
            (start, start + (event.end - event.start), None)
        };
        count += 1;
        let (s, e) = all_day.unwrap_or_else(|| {
            (
                start.with_timezone(&tz).date_naive(),
                (end - Duration::seconds(1)).with_timezone(&tz).date_naive() + Duration::days(1),
            )
        });
        if e > from && s < to {
            // Clone only occurrences inside the window.
            let mut instance = event.clone();
            instance.start = start;
            instance.end = end;
            instance.all_day = all_day;
            instance.series_id = Some(event.id.clone());
            instance.repeat = Repeat::Never;
            out.push(instance);
        }
    }
    out
}
#[derive(Clone, Debug)]
pub struct Placement {
    pub index: usize,
    pub column: usize,
    pub columns: usize,
    pub start: f32,
    pub end: f32,
}
/// Assign equal-width lanes per connected overlap group; touching events don't overlap.
pub fn lanes(intervals: &[(f32, f32)]) -> Vec<Placement> {
    let mut order: Vec<_> = (0..intervals.len()).collect();
    order.sort_by(|&a, &b| {
        intervals[a]
            .0
            .total_cmp(&intervals[b].0)
            .then(intervals[b].1.total_cmp(&intervals[a].1))
    });
    let mut out = vec![];
    let mut group = vec![];
    let mut ends: Vec<f32> = vec![];
    let mut group_end = f32::NEG_INFINITY;
    for index in order {
        let (start, end) = intervals[index];
        if start >= group_end && !group.is_empty() {
            finish_group(&mut out, &mut group, ends.len());
            ends.clear();
        }
        let column = ends.iter().position(|&v| v <= start).unwrap_or(ends.len());
        if column == ends.len() {
            ends.push(end)
        } else {
            ends[column] = end
        };
        group_end = if group.is_empty() {
            end
        } else {
            group_end.max(end)
        };
        group.push(Placement {
            index,
            column,
            columns: 0,
            start,
            end,
        });
    }
    finish_group(&mut out, &mut group, ends.len());
    out.sort_by_key(|p| p.index);
    out
}
fn finish_group(out: &mut Vec<Placement>, group: &mut Vec<Placement>, columns: usize) {
    for mut p in group.drain(..) {
        p.columns = columns;
        out.push(p)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlap_groups_and_touching_edges() {
        let p = lanes(&[(9., 10.), (9.5, 11.), (10., 10.5), (11., 12.)]);
        assert_eq!(
            p.iter().map(|p| (p.column, p.columns)).collect::<Vec<_>>(),
            vec![(0, 2), (1, 2), (0, 2), (0, 1)]
        );
    }
    #[test]
    fn weekly_keeps_wall_clock_across_dst() {
        let mut e = Event::new(
            "local",
            NaiveDate::from_ymd_opt(2026, 10, 19).unwrap(),
            "Europe/London",
        )
        .unwrap();
        e.title = "Meeting".into();
        e.repeat = Repeat::Weekly;
        e.repeat_count = Some(3);
        let o = occurrences(&e, e.date(), e.date() + Duration::days(30));
        assert_eq!(o.len(), 3);
        assert_eq!(
            o[1].start.timestamp() - o[0].start.timestamp(),
            7 * 86400 + 3600
        );
        assert_eq!(
            o[1].start
                .with_timezone(&zone("Europe/London"))
                .format("%H:%M")
                .to_string(),
            "09:00"
        );
    }
    #[test]
    fn monthly_skips_invalid_dates_and_exclusive_until() {
        let mut e = Event::new(
            "local",
            NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            "UTC",
        )
        .unwrap();
        e.repeat = Repeat::Monthly;
        e.repeat_count = Some(2);
        assert_eq!(
            occurrences(&e, e.date(), e.date() + Duration::days(130)).len(),
            2
        );
    }
    #[test]
    fn long_running_series_expand_only_the_requested_window() {
        let ymd = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        for (repeat, first, from, to, expected) in [
            (
                Repeat::Daily,
                ymd(2016, 1, 1),
                ymd(2026, 10, 5),
                ymd(2026, 10, 12),
                (5..12).map(|d| ymd(2026, 10, d)).collect::<Vec<_>>(),
            ),
            (
                Repeat::Weekly,
                ymd(2016, 1, 4),
                ymd(2026, 10, 1),
                ymd(2026, 10, 15),
                vec![ymd(2026, 10, 5), ymd(2026, 10, 12)],
            ),
            (
                Repeat::Monthly,
                ymd(2020, 1, 31),
                ymd(2026, 10, 1),
                ymd(2026, 12, 1),
                vec![ymd(2026, 10, 31)],
            ),
            (
                Repeat::Yearly,
                ymd(2000, 2, 29),
                ymd(2028, 2, 1),
                ymd(2028, 3, 1),
                vec![ymd(2028, 2, 29)],
            ),
        ] {
            let mut e = Event::new("local", first, "Europe/London").unwrap();
            e.repeat = repeat;
            let dates: Vec<_> = occurrences(&e, from, to).iter().map(|o| o.date()).collect();
            assert_eq!(dates, expected, "{repeat:?}");
            // An all-day series spanning several days still appears on its later days.
            e.all_day = Some((first, first + Duration::days(3)));
            assert!(
                occurrences(&e, from, to)
                    .iter()
                    .all(|o| o.dates_in(zone("UTC")).1 > from)
            );
        }
        let mut overnight = Event::new("local", ymd(2016, 1, 1), "UTC").unwrap();
        overnight.repeat = Repeat::Daily;
        overnight.end = overnight.start + Duration::hours(20);
        let o = occurrences(&overnight, ymd(2026, 10, 5), ymd(2026, 10, 6));
        assert_eq!(
            o.iter().map(|o| o.date()).collect::<Vec<_>>(),
            vec![ymd(2026, 10, 4), ymd(2026, 10, 5)]
        );
    }
    #[test]
    fn reject_dst_ambiguity() {
        assert!(
            local_datetime(
                NaiveDate::from_ymd_opt(2026, 10, 25)
                    .unwrap()
                    .and_hms_opt(1, 30, 0)
                    .unwrap(),
                "Europe/London"
            )
            .is_err()
        );
    }
}
