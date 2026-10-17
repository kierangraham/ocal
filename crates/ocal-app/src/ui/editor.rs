//! Event editor: a floating card with date pickers and typed, steppable times.
use super::*;
use crate::appearance::{
    RADIUS_SMALL, Tones, card_shadow, mode_button, pill_tab, send_button, shadow,
};
use gpui_kit::base::CalendarEvent;
use gpui_omarchy::{DatePickerState, combobox, date_picker, switch};

const REPEATS: [Repeat; 5] = [
    Repeat::Never,
    Repeat::Daily,
    Repeat::Weekly,
    Repeat::Monthly,
    Repeat::Yearly,
];
const MEETINGS: [&str; 3] = ["No meeting", "Paste a link", "Google Meet"];
/// Up to this many calendars are offered as tabs; more get a list.
const CALENDAR_TABS: usize = 4;

pub(super) struct Editor {
    pub(super) base: Event,
    pub(super) title: Entity<InputState>,
    pub(super) location: Entity<InputState>,
    pub(super) link: Entity<InputState>,
    pub(super) guests: Entity<InputState>,
    pub(super) start_time: Entity<InputState>,
    pub(super) end_time: Entity<InputState>,
    pub(super) start_date: Entity<DatePickerState>,
    pub(super) end_date: Entity<DatePickerState>,
    pub(super) timezone: Entity<ChoiceState>,
    pub(super) notes: Entity<TextareaState>,
    pub(super) calendar: Entity<ChoiceState>,
    pub(super) repeat: Entity<ChoiceState>,
    pub(super) all_day: bool,
    /// 0: none, 1: pasted link, 2: generate Google Meet.
    pub(super) meeting: usize,
    pub(super) meet_request: String,
    pub(super) notify: bool,
    pub(super) details: bool,
    /// The end date is shown even though it matches the start.
    pub(super) end_shown: bool,
    pub(super) editing: bool,
    pub(super) discard: bool,
    /// The draft as opened, for the unsaved-changes check.
    initial: String,
    /// The last valid start, so moving it carries the end along.
    start_at: Option<NaiveDateTime>,
    pub(super) focus: FocusHandle,
    last_focus: Option<usize>,
}

impl Editor {
    fn date(state: &Entity<DatePickerState>, cx: &App) -> Option<NaiveDate> {
        state.read(cx).calendar.read(cx).date().start()
    }
    pub(super) fn start_date(&self, cx: &App) -> Option<NaiveDate> {
        Self::date(&self.start_date, cx)
    }
    pub(super) fn end_date(&self, cx: &App) -> Option<NaiveDate> {
        Self::date(&self.end_date, cx)
    }
    pub(super) fn start_time(&self, cx: &App) -> Option<NaiveTime> {
        parse_time(&self.start_time.read(cx).value())
    }
    pub(super) fn end_time(&self, cx: &App) -> Option<NaiveTime> {
        parse_time(&self.end_time.read(cx).value())
    }
    /// The draft's span in its own zone, for previewing it in the grid.
    pub(super) fn span(&self, cx: &App) -> Option<(chrono::DateTime<Utc>, chrono::DateTime<Utc>)> {
        let tz = self
            .timezone
            .read(cx)
            .selected()
            .map(|z| z.value.to_string())
            .unwrap_or_else(|| self.base.timezone.clone());
        if self.all_day {
            return None;
        }
        let (s, e) = (self.start(cx)?, self.end(cx)?);
        let (s, e) = (local_datetime(s, &tz).ok()?, local_datetime(e, &tz).ok()?);
        (e > s).then_some((s, e))
    }
    fn start(&self, cx: &App) -> Option<NaiveDateTime> {
        Some(self.start_date(cx)?.and_time(self.start_time(cx)?))
    }
    fn end(&self, cx: &App) -> Option<NaiveDateTime> {
        Some(self.end_date(cx)?.and_time(self.end_time(cx)?))
    }
    /// Text inputs in visual order, for disabling while saving and focus reveal.
    fn inputs(&self) -> [&Entity<InputState>; 6] {
        [
            &self.title,
            &self.start_time,
            &self.end_time,
            &self.location,
            &self.link,
            &self.guests,
        ]
    }
    pub(super) fn set_disabled(&self, disabled: bool, cx: &mut App) {
        for field in self.inputs() {
            field.update(cx, |s, cx| s.set_disabled(disabled, cx));
        }
        self.notes.update(cx, |s, cx| s.set_disabled(disabled, cx));
    }
    /// Everything the user can change, as one comparable string.
    fn fingerprint(&self, cx: &App) -> String {
        let choice = |c: &Entity<ChoiceState>| {
            c.read(cx)
                .selected()
                .map(|s| s.value.to_string())
                .unwrap_or_default()
        };
        format!(
            "{:?}|{:?}|{:?}|{:?}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.start_date(cx),
            self.end_date(cx),
            self.start_time.read(cx).value(),
            self.end_time.read(cx).value(),
            self.title.read(cx).value(),
            self.location.read(cx).value(),
            self.link.read(cx).value(),
            self.guests.read(cx).value(),
            self.notes.read(cx).value(),
            choice(&self.calendar),
            choice(&self.repeat),
            choice(&self.timezone),
            self.all_day,
            self.meeting,
        )
    }
}

/// Read a loosely typed time: "9", "930", "09:30", "9.30", "2pm", "12:15am".
pub(super) fn parse_time(text: &str) -> Option<NaiveTime> {
    let text = text.trim().to_ascii_lowercase();
    let (text, meridiem) = if let Some(t) = text.strip_suffix("am") {
        (t.trim(), Some(false))
    } else if let Some(t) = text.strip_suffix("pm") {
        (t.trim(), Some(true))
    } else {
        (text.as_str(), None)
    };
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty()
        || text
            .chars()
            .any(|c| !c.is_ascii_digit() && c != ':' && c != '.')
    {
        return None;
    }
    let (hour, minute) = if text.contains([':', '.']) {
        let (h, m) = text.split_once([':', '.'])?;
        (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?)
    } else {
        match digits.len() {
            1 | 2 => (digits.parse().ok()?, 0),
            3 | 4 => {
                let split = digits.len() - 2;
                (digits[..split].parse().ok()?, digits[split..].parse().ok()?)
            }
            _ => return None,
        }
    };
    let hour = match meridiem {
        Some(_) if !(1..=12).contains(&hour) => return None,
        Some(pm) => hour % 12 + if pm { 12 } else { 0 },
        None => hour,
    };
    NaiveTime::from_hms_opt(hour, minute, 0)
}

/// "45 min", "1 h", "1 h 30 min" or "2 d".
fn duration_label(d: Duration) -> String {
    let minutes = d.num_minutes();
    if minutes <= 0 {
        String::new()
    } else if minutes % 1440 == 0 {
        format!("{} d", minutes / 1440)
    } else if minutes < 60 {
        format!("{minutes} min")
    } else if minutes % 60 == 0 {
        format!("{} h", minutes / 60)
    } else {
        format!("{} h {} min", minutes / 60, minutes % 60)
    }
}

impl Ocal {
    pub(super) fn open_editor(
        &mut self,
        mut e: Event,
        editing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        if editing
            && e.remote_id.is_none()
            && e.series_id.is_some()
            && let Some(master) = self.snapshot.events.iter().find(|x| x.id == e.id)
        {
            e = master.clone();
        }
        let tz = zone(&e.timezone);
        let start = e.start.with_timezone(&tz);
        let end = e.end.with_timezone(&tz);
        let (s, t) = e
            .all_day
            .map(|(s, t)| (s, t - Duration::days(1)))
            .unwrap_or((start.date_naive(), end.date_naive()));
        let mut text = |value: String, placeholder: &'static str, cx: &mut Context<Self>| {
            let field = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(value)
                    .placeholder(placeholder)
            });
            cx.subscribe(&field, |_, _, _: &InputEvent, cx| cx.notify())
                .detach();
            field
        };
        let title = text(
            e.title.clone(),
            if e.task { "New task" } else { "New event" },
            cx,
        );
        let location = text(e.location.clone(), "Add a location", cx);
        let link = text(e.link.clone(), "https://…", cx);
        let guests = text(
            e.attendees
                .iter()
                .map(|a| a.email.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            "Guests’ email addresses",
            cx,
        );
        let start_time = text(start.format("%H:%M").to_string(), "09:00", cx);
        let end_time = text(end.format("%H:%M").to_string(), "10:00", cx);
        let mut picker = |date: NaiveDate, cx: &mut Context<Self>| {
            let state = cx.new(|cx| DatePickerState::new(window, cx));
            let calendar = state.read(cx).calendar.clone();
            calendar.update(cx, |c, cx| c.set_date(date, window, cx));
            cx.observe(&state, |_, _, cx| cx.notify()).detach();
            state
        };
        let start_date = picker(s, cx);
        let end_date = picker(t, cx);
        // Moving the start keeps the event's length by moving the end with it.
        cx.subscribe_in(
            &start_time,
            window,
            |app, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    app.carry_end(window, cx);
                }
            },
        )
        .detach();
        let start_calendar = start_date.read(cx).calendar.clone();
        cx.subscribe_in(
            &start_calendar,
            window,
            |app, _, _: &CalendarEvent, window, cx| app.carry_end(window, cx),
        )
        .detach();
        let calendars: Vec<_> = self
            .editor_calendars(&e)
            .into_iter()
            .map(|c| ChoiceItem::new(c.id.clone(), c.display_name()))
            .collect();
        let index = calendars
            .iter()
            .position(|c| c.value.as_ref() == e.calendar)
            .unwrap_or(0);
        let calendar = cx.new(|cx| {
            ChoiceState::new(calendars, window, cx)
                .label("Calendar")
                .default_selected(index)
        });
        let repeat = cx.new(|cx| {
            ChoiceState::new(
                REPEATS
                    .iter()
                    .enumerate()
                    .map(|(i, r)| ChoiceItem::new(i.to_string(), r.label()))
                    .collect(),
                window,
                cx,
            )
            .label("Repeat")
            .default_selected(REPEATS.iter().position(|r| *r == e.repeat).unwrap_or(0))
        });
        // The event's own zone and the desktop zone lead the searchable list.
        let zones = zone_names(&[e.timezone.clone(), system_zone()]);
        let timezone = cx.new(|cx| {
            ChoiceState::new(zone_items(&zones), window, cx)
                .label("Time zone")
                .default_selected(0)
        });
        for choice in [&calendar, &repeat, &timezone] {
            cx.observe(choice, |_, _, cx| cx.notify()).detach();
        }
        let notes = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(4)
                .default_value(e.notes.clone())
                .placeholder("Add notes")
        });
        cx.subscribe(&notes, |_, _, _: &InputEvent, cx| cx.notify())
            .detach();
        title.update(cx, |s, cx| s.focus(window, cx));
        let all_day = e.all_day.is_some();
        let meeting = usize::from(!e.link.is_empty());
        self.form_scroll.set_offset(point(px(0.), px(0.)));
        let mut editor = Editor {
            base: e,
            title,
            location,
            link,
            guests,
            start_time,
            end_time,
            start_date,
            end_date,
            timezone,
            notes,
            calendar,
            repeat,
            all_day,
            meeting,
            meet_request: uuid::Uuid::new_v4().simple().to_string(),
            notify: true,
            details: false,
            end_shown: false,
            editing,
            discard: false,
            initial: String::new(),
            start_at: None,
            focus: cx.focus_handle(),
            last_focus: None,
        };
        editor.initial = editor.fingerprint(cx);
        editor.start_at = editor.start(cx);
        self.editor = Some(editor);
        self.selected = None;
        self.settings = false;
        self.help = false;
        self.sidebar_open = false;
        self.keys_open = false;
        self.editor_error = None;
        cx.notify();
    }
    /// Shift the end by however far the start just moved.
    fn carry_end(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(f) = self.editor.as_mut() else {
            return;
        };
        let (Some(before), Some(after)) = (f.start_at, f.start(cx)) else {
            return;
        };
        if before == after {
            return;
        }
        f.start_at = Some(after);
        if let Some(end) = f.end(cx) {
            let moved = end + (after - before);
            let calendar = f.end_date.read(cx).calendar.clone();
            calendar.update(cx, |c, cx| c.set_date(moved.date(), window, cx));
            f.end_time.update(cx, |s, cx| {
                s.set_value(moved.format("%H:%M").to_string(), window, cx)
            });
        }
        cx.notify();
    }
    /// Arrow keys step a focused time field by 15 minutes.
    fn step_time(
        &mut self,
        start: bool,
        minutes: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(f) = self.editor.as_ref() else {
            return;
        };
        let (field, date) = if start {
            (f.start_time.clone(), f.start_date.clone())
        } else {
            (f.end_time.clone(), f.end_date.clone())
        };
        let Some(time) = parse_time(&field.read(cx).value()) else {
            return;
        };
        // Snap to the quarter hour before stepping off it.
        let minute = (time.num_seconds_from_midnight() / 60) as i64;
        let snapped = if minute % 15 == 0 {
            minute + minutes
        } else if minutes > 0 {
            minute + 15 - minute % 15
        } else {
            minute - minute % 15
        };
        let days = snapped.div_euclid(1440);
        let next = NaiveTime::from_num_seconds_from_midnight_opt(
            (snapped.rem_euclid(1440) * 60) as u32,
            0,
        )
        .unwrap_or(time);
        if days != 0
            && let Some(d) = Editor::date(&date, cx)
        {
            let calendar = date.read(cx).calendar.clone();
            calendar.update(cx, |c, cx| c.set_date(d + Duration::days(days), window, cx));
        }
        field.update(cx, |s, cx| {
            s.set_value(next.format("%H:%M").to_string(), window, cx)
        });
        if start {
            self.carry_end(window, cx);
        }
        cx.notify();
    }
    pub(super) fn draft(&self, cx: &App) -> anyhow::Result<Event> {
        let f = self
            .editor
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No event editor"))?;
        let value = |field: &Entity<InputState>| field.read(cx).value().trim().to_owned();
        let mut e = f.base.clone();
        e.title = value(&f.title);
        e.calendar = f
            .calendar
            .read(cx)
            .selected()
            .ok_or_else(|| anyhow::anyhow!("Choose a writable calendar"))?
            .value
            .to_string();
        e.timezone = f
            .timezone
            .read(cx)
            .selected()
            .map(|z| z.value.to_string())
            .unwrap_or_else(|| f.base.timezone.clone());
        let s = f
            .start_date(cx)
            .ok_or_else(|| anyhow::anyhow!("Choose a start date"))?;
        let t = f
            .end_date(cx)
            .ok_or_else(|| anyhow::anyhow!("Choose an end date"))?;
        if f.all_day {
            e.all_day = Some((s, t + Duration::days(1)));
            e.start = s.and_hms_opt(0, 0, 0).unwrap().and_utc();
            e.end = (t + Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc();
        } else {
            e.all_day = None;
            let start = f
                .start_time(cx)
                .ok_or_else(|| anyhow::anyhow!("Use a start time like 9:15 or 2pm"))?;
            let end = f
                .end_time(cx)
                .ok_or_else(|| anyhow::anyhow!("Use an end time like 10:00 or 3pm"))?;
            e.start = local_datetime(s.and_time(start), &e.timezone)?;
            e.end = local_datetime(t.and_time(end), &e.timezone)?;
        }
        e.location = value(&f.location);
        e.notes = f.notes.read(cx).value().to_string();
        e.link = if f.meeting == 1 {
            value(&f.link)
        } else {
            String::new()
        };
        e.clear_conference = f.base.has_conference && (f.meeting != 1 || e.link != f.base.link);
        if !f.base.link.is_empty() && (f.meeting != 1 || e.link != f.base.link) {
            // Remove only the standalone meeting-link line previously added to notes.
            e.notes = e
                .notes
                .lines()
                .filter(|line| line.trim() != f.base.link)
                .collect::<Vec<_>>()
                .join("\n")
                .trim_end()
                .to_owned();
        }
        e.meet_request = (f.meeting == 2).then(|| f.meet_request.clone());
        if e.meet_request.is_some() {
            anyhow::ensure!(
                self.snapshot
                    .calendars
                    .iter()
                    .any(|c| c.id == e.calendar && c.account.is_some() && c.meet_available),
                "Choose a Google calendar that supports Meet"
            );
        }
        e.attendees = value(&f.guests)
            .split([',', ';'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|email| {
                f.base
                    .attendees
                    .iter()
                    .find(|a| a.email.eq_ignore_ascii_case(email))
                    .cloned()
                    .unwrap_or(ocal_core::Attendee {
                        email: email.into(),
                        response: "needsAction".into(),
                        me: false,
                    })
            })
            .collect();
        if e.series_id.is_none() {
            e.repeat = REPEATS[f
                .repeat
                .read(cx)
                .selected()
                .and_then(|s| s.value.parse::<usize>().ok())
                .unwrap_or(0)];
        }
        e.validate()?;
        Ok(e)
    }
    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        match self.draft(cx) {
            Ok(event) => {
                let notify = self.editor.as_ref().is_some_and(|f| f.notify);
                self.busy = true;
                if let Some(f) = &self.editor {
                    f.set_disabled(true, cx);
                }
                self.editor_error = None;
                self.dispatch(Command::Save { event, notify });
            }
            Err(e) => self.editor_error = Some(format!("{e:#}")),
        }
        cx.notify();
    }
    pub(super) fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Some(f) = &mut self.editor
            && f.fingerprint(cx) != f.initial
        {
            f.discard = true;
            cx.notify();
            return;
        }
        self.editor = None;
        self.selection.anchor = None;
        self.editor_error = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }
    pub(super) fn editor_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        let busy = self.busy;
        let Some(f) = self.editor.as_mut() else {
            return div().into_any_element();
        };
        let focused = f
            .inputs()
            .iter()
            .position(|s| s.read(cx).focus_handle(cx).is_focused(window))
            .or_else(|| {
                f.notes
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
                    .then_some(9)
            });
        let reveal = if focused != f.last_focus {
            focused
        } else {
            None
        };
        f.last_focus = focused;
        let f = self.editor.as_ref().unwrap();
        let scroll = self.form_scroll.clone();
        let clear = t.foreground.opacity(0.);
        // Borderless inputs: the card, not each field, is the frame.
        let field = |i: usize,
                     state: &Entity<InputState>,
                     selector: &'static str,
                     window: &mut Window,
                     cx: &mut Context<Self>| {
            reveal_field(
                input(selector, state, window, cx)
                    .debug_selector(move || selector.into())
                    .w_full()
                    .py(rems(0.22))
                    .border_0()
                    .bg(clear),
                reveal == Some(i),
                &scroll,
            )
        };
        let line = |glyph: IconName, content: AnyElement| {
            div()
                .flex()
                .items_center()
                .gap(rems(0.625))
                .min_h(rems(2.25))
                .px(rems(0.375))
                .child(
                    icon(glyph)
                        .size(rems(1.))
                        .flex_shrink_0()
                        .text_color(t.muted()),
                )
                .child(div().flex_1().min_w_0().child(content))
        };
        let note = |text: &'static str| {
            div()
                .text_size(rems(0.75))
                .text_color(t.muted())
                .whitespace_normal()
                .child(text)
                .into_any_element()
        };
        let selected_calendar = f
            .calendar
            .read(cx)
            .selected()
            .map(|c| c.value.to_string())
            .unwrap_or_default();
        let google = self
            .snapshot
            .calendars
            .iter()
            .any(|c| c.id == selected_calendar && c.account.is_some());
        let can_meet = self.snapshot.calendars.iter().any(|c| {
            c.id == selected_calendar && c.account.is_some() && c.meet_available && c.writable
        });
        // A time chip that steps by 15 minutes with the arrow keys.
        let time = |start: bool, window: &mut Window, cx: &mut Context<Self>| {
            let (i, state, selector) = if start {
                (1, &f.start_time, "editor-start-time")
            } else {
                (2, &f.end_time, "editor-end-time")
            };
            div()
                .w(rems(4.25))
                .flex_shrink_0()
                .px(rems(0.25))
                .rounded(RADIUS_SMALL)
                .bg(t.hover())
                .capture_key_down(cx.listener(
                    move |app, event: &gpui_kit::KeyDownEvent, window, cx| {
                        let m = &event.keystroke.modifiers;
                        if m.control || m.alt || m.shift || m.platform {
                            return;
                        }
                        let step = match event.keystroke.key.as_str() {
                            "up" => 15,
                            "down" => -15,
                            _ => return,
                        };
                        app.step_time(start, step, window, cx);
                        cx.stop_propagation();
                    },
                ))
                .child(field(i, state, selector, window, cx))
        };
        let date = |start: bool, cx: &mut Context<Self>| {
            let picker = if start { &f.start_date } else { &f.end_date };
            div()
                .w(rems(8.5))
                .flex_shrink_0()
                .debug_selector(move || {
                    if start {
                        "editor-start-date"
                    } else {
                        "editor-end-date"
                    }
                    .into()
                })
                .child(date_picker(
                    if start { "start-date" } else { "end-date" },
                    picker,
                    cx,
                ))
        };
        let length = (!f.all_day)
            .then(|| Some(f.end(cx)? - f.start(cx)?))
            .flatten()
            .map(duration_label)
            .unwrap_or_default();
        let mut when = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(rems(0.375))
            .child(date(true, cx));
        if !f.all_day {
            when = when.child(time(true, window, cx));
            if !f.base.task {
                when = when
                    .child(
                        div()
                            .text_size(rems(0.75))
                            .text_color(t.muted())
                            .child("to"),
                    )
                    .child(time(false, window, cx));
            }
        }
        if !length.is_empty() && !f.base.task {
            when = when.child(
                div()
                    .pl(rems(0.25))
                    .text_size(rems(0.75))
                    .text_color(t.muted())
                    .child(length.clone()),
            );
        }
        let all_day = {
            let target = cx.entity().downgrade();
            switch("all-day", "All day", f.all_day, cx)
                .debug_selector(|| "all-day".into())
                .disabled(busy)
                .on_change(move |checked, _, _, cx| {
                    let _ = target.update(cx, |app, cx| {
                        if let Some(f) = &mut app.editor {
                            f.all_day = checked;
                        }
                        cx.notify();
                    });
                })
        };
        // The end date only takes room when it can differ from the start.
        let show_end =
            !f.base.task && (f.all_day || f.end_shown || f.start_date(cx) != f.end_date(cx));
        let span = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(rems(0.75))
            .pl(rems(2.))
            .pb(rems(0.25))
            .child(all_day)
            .map(|row| {
                if show_end {
                    row.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(rems(0.375))
                            .child(
                                div()
                                    .text_size(rems(0.75))
                                    .text_color(t.muted())
                                    .child("ends"),
                            )
                            .child(date(false, cx)),
                    )
                } else if f.base.task {
                    row
                } else {
                    row.child(
                        mode_button("show-end-date", "Ends another day", None, cx)
                            .disabled(busy)
                            .on_click(cx.listener(|app, _, _, cx| {
                                if let Some(f) = &mut app.editor {
                                    f.end_shown = true;
                                }
                                cx.notify();
                            })),
                    )
                }
            });
        let calendars = self.editor_calendars(&f.base).len();
        let calendar = if busy {
            div()
                .child(
                    f.calendar
                        .read(cx)
                        .selected()
                        .map(|s| s.label.clone())
                        .unwrap_or_default(),
                )
                .into_any_element()
        } else if calendars <= CALENDAR_TABS {
            let options = self.editor_calendars(&f.base);
            div()
                .flex()
                .flex_wrap()
                .gap(rems(0.25))
                .children(options.into_iter().enumerate().map(|(i, c)| {
                    let state = f.calendar.clone();
                    pill_tab(("calendar-choice", i), "", c.id == selected_calendar, cx)
                        .debug_selector(move || format!("calendar-choice-{i}"))
                        .child(
                            div()
                                .size(rems(0.4375))
                                .rounded_full()
                                .bg(self.calendar_color(&c.id, cx)),
                        )
                        .child(c.name.clone())
                        .on_click(cx.listener(move |_, _, _, cx| {
                            state.update(cx, |s, cx| s.set_selected(Some(i), cx));
                            cx.notify();
                        }))
                }))
                .into_any_element()
        } else {
            select("editor-calendar", &f.calendar, window, cx).into_any_element()
        };
        let meeting = div().flex().flex_wrap().gap(rems(0.25)).children(
            MEETINGS.into_iter().enumerate().map(|(i, name)| {
                pill_tab(("meeting", i), name, f.meeting == i, cx)
                    .debug_selector(move || format!("meeting-{i}"))
                    .disabled(busy || (i == 2 && !can_meet))
                    .on_click(cx.listener(move |app, _, window, cx| {
                        if let Some(f) = &mut app.editor {
                            f.meeting = i;
                            if i == 1 {
                                f.link.update(cx, |s, cx| s.focus(window, cx));
                            }
                        }
                        cx.notify();
                    }))
            }),
        );
        let mut body = div()
            .id("editor-scroll")
            .debug_selector(|| "editor-scroll".into())
            .flex_shrink(1.)
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.form_scroll)
            .px(rems(0.75))
            .pb(rems(0.5))
            .flex()
            .flex_col()
            .child(line(IconName::Clock, when.into_any_element()))
            .child(span)
            .child(line(IconName::Calendar, calendar))
            .child(line(
                IconName::MapPin,
                field(3, &f.location, "editor-location", window, cx).into_any_element(),
            ));
        if !f.base.task {
            body = body.child(line(IconName::Video, meeting.into_any_element()));
            if f.meeting == 1 {
                body = body.child(line(
                    IconName::Link,
                    field(4, &f.link, "editor-link", window, cx).into_any_element(),
                ));
            } else if f.meeting == 2 {
                body = body.child(line(
                    IconName::Video,
                    note("Google creates the Meet link when you save."),
                ));
            }
        }
        if f.base.series_id.is_some() {
            body = body.child(line(
                IconName::Repeat,
                note(if f.base.remote_id.is_some() {
                    "Changes apply to this occurrence."
                } else {
                    "Changes apply to the entire series."
                }),
            ));
        }
        if f.details {
            if google && !f.base.task {
                body = body.child(line(
                    IconName::Users,
                    field(5, &f.guests, "editor-guests", window, cx).into_any_element(),
                ));
                if !f.guests.read(cx).value().is_empty() {
                    let target = cx.entity().downgrade();
                    body = body.child(
                        div().pl(rems(2.)).pb(rems(0.25)).child(
                            switch("notify-guests", "Email guests", f.notify, cx)
                                .debug_selector(|| "notify-guests".into())
                                .disabled(busy)
                                .on_change(move |checked, _, _, cx| {
                                    let _ = target.update(cx, |app, cx| {
                                        if let Some(f) = &mut app.editor {
                                            f.notify = checked;
                                        }
                                        cx.notify();
                                    });
                                }),
                        ),
                    );
                }
            }
            if f.base.series_id.is_none() {
                body = body.child(line(
                    IconName::Repeat,
                    if busy {
                        div().into_any_element()
                    } else {
                        select("editor-repeat", &f.repeat, window, cx).into_any_element()
                    },
                ));
            }
            body = body
                .child(line(
                    IconName::Globe,
                    combobox("editor-timezone", &f.timezone, window, cx).into_any_element(),
                ))
                .child(
                    div()
                        .flex()
                        .gap(rems(0.625))
                        .px(rems(0.375))
                        .pt(rems(0.5))
                        .child(
                            icon(IconName::TextAlignStart)
                                .size(rems(1.))
                                .mt(rems(0.25))
                                .flex_shrink_0()
                                .text_color(t.muted()),
                        )
                        .child(reveal_field(
                            textarea("editor-notes", &f.notes, window, cx)
                                .debug_selector(|| "editor-notes".into())
                                .disabled(busy)
                                .w_full()
                                .border_0()
                                .bg(clear),
                            reveal == Some(9),
                            &scroll,
                        )),
                );
        }
        let label = match (f.editing, f.base.task) {
            (true, true) => "Edit task",
            (true, false) => "Edit event",
            (false, true) => "New task",
            (false, false) => "New event",
        };
        let save_label = if busy {
            "Saving…"
        } else if google && f.notify && !f.guests.read(cx).value().trim().is_empty() {
            "Save & invite"
        } else {
            "Save"
        };
        let untitled = f.title.read(cx).value().trim().is_empty();
        let error = self.editor_error.clone().filter(|_| !busy).map(|error| {
            div()
                .debug_selector(|| "editor-error".into())
                .mx(rems(1.125))
                .mb(rems(0.5))
                .px(rems(0.625))
                .py(rems(0.5))
                .rounded(RADIUS_SMALL)
                .bg(t.danger.opacity(0.08))
                .text_color(t.danger)
                .text_size(rems(0.75))
                .whitespace_normal()
                .child(error)
        });
        let footer = if f.discard {
            div()
                .flex()
                .items_center()
                .gap(rems(0.375))
                .child(
                    div()
                        .flex_1()
                        .pl(rems(0.375))
                        .child("Discard unsaved changes?"),
                )
                .child(
                    mode_button("discard-editor", "Discard", None, cx)
                        .hover(|s| s.bg(t.danger.opacity(0.1)).text_color(t.danger))
                        .on_click(cx.listener(|app, _, window, cx| {
                            app.editor = None;
                            app.selection.anchor = None;
                            app.editor_error = None;
                            window.focus(&app.focus, cx);
                            cx.notify();
                        })),
                )
                .child(
                    send_button("keep-editing", "Keep editing", None, None, cx).on_click(
                        cx.listener(|app, _, _, cx| {
                            if let Some(f) = &mut app.editor {
                                f.discard = false;
                            }
                            cx.notify();
                        }),
                    ),
                )
        } else {
            div()
                .flex()
                .items_center()
                .gap(rems(0.125))
                .child(
                    mode_button(
                        "more-details",
                        if f.details {
                            "Fewer details"
                        } else {
                            "More details"
                        },
                        Some(IconName::TextAlignStart),
                        cx,
                    )
                    .debug_selector(|| "more-details".into())
                    .disabled(busy)
                    .on_click(cx.listener(|app, _, _, cx| {
                        if let Some(f) = &mut app.editor {
                            f.details = !f.details;
                        }
                        cx.notify();
                    })),
                )
                .child(div().flex_1())
                .child(
                    mode_button("cancel-editor", "Cancel", None, cx)
                        .disabled(busy)
                        .on_click(cx.listener(|app, _, window, cx| app.close_editor(window, cx))),
                )
                .child(
                    send_button("save-event", save_label, None, Some("ctrl ↵"), cx)
                        .debug_selector(|| "save-event".into())
                        .ml(rems(0.25))
                        .disabled(busy)
                        // Like Omail's Send: dimmed until there is something to save.
                        .when(untitled, |b| b.opacity(0.45))
                        .on_click(cx.listener(|app, _, _, cx| app.save(cx))),
                )
        };
        let title = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(rems(0.5))
            .pl(rems(0.75))
            .pr(rems(0.625))
            .pt(rems(0.75))
            .pb(rems(0.25))
            .child(
                div().flex_1().min_w_0().child(reveal_field(
                    input("editor-title", &f.title, window, cx)
                        .debug_selector(|| "editor-title".into())
                        .w_full()
                        .border_0()
                        .bg(clear)
                        .text_size(rems(1.125))
                        .font_weight(FontWeight::SEMIBOLD),
                    reveal == Some(0),
                    &scroll,
                )),
            )
            .child(
                tool_button("close-editor", IconName::X, "Close editor", "Esc", cx)
                    .size(rems(1.75))
                    .rounded(RADIUS_SMALL)
                    .disabled(busy)
                    .on_click(cx.listener(|app, _, window, cx| app.close_editor(window, cx))),
            );
        let card = focus_scope("event-editor")
            .debug_selector(|| "event-editor".into())
            .role(gpui_kit::Role::Dialog)
            .aria_label(label)
            .key_context("EventEditor")
            .track_focus(&f.focus)
            .absolute()
            .flex()
            .flex_col()
            .bg(t.background)
            .occlude()
            .child(title)
            .child(body)
            .children(error)
            .child(
                footer
                    .flex_shrink_0()
                    .px(rems(0.5))
                    .py(rems(0.5))
                    .border_t_1()
                    .border_color(t.hair())
                    .text_size(rems(0.78))
                    .text_color(t.muted()),
            );
        let viewport = window.viewport_size();
        let rem = window.rem_size();
        if self.compact {
            return card
                .left_0()
                .right_0()
                .top(rems(HEADER))
                .bottom(rems(SHEET_GAP))
                .border_b_1()
                .border_color(t.hair())
                .shadow(card_shadow(&t))
                .into_any_element();
        }
        // A card centred over the sheet, just below its toolbar, ringed in the
        // accent colour while a field has focus.
        let gap = rems(SHEET_GAP).to_pixels(rem);
        let sheet_left = if self.docked_sidebar(window) {
            rems(SIDEBAR).to_pixels(rem)
        } else {
            gap
        };
        let sheet_right = viewport.width - gap;
        let width = rems(COMPOSER_WIDTH)
            .to_pixels(rem)
            .min(sheet_right - sheet_left - gap * 2.);
        let top = rems(SHEET_GAP + TOOLBAR + 0.375).to_pixels(rem);
        let mut shadows = card_shadow(&t);
        if focused.is_some() {
            shadows.push(shadow(t.accent, 0., 0., 1.5));
        }
        card.left(sheet_left + (sheet_right - sheet_left - width) / 2.)
            .top(top)
            .w(width)
            .max_h(viewport.height - top - gap * 2.)
            .rounded(rems(0.875))
            .shadow(shadows)
            .into_any_element()
    }
    /// Calendars a draft can be saved to, in the order the calendar choice lists them.
    pub(super) fn editor_calendars(&self, e: &Event) -> Vec<&ocal_core::Calendar> {
        self.snapshot
            .calendars
            .iter()
            .filter(|c| {
                c.writable
                    && (!e.task || c.account.is_none())
                    && (e.remote_id.is_none() || c.id == e.calendar)
            })
            .collect()
    }
}

/// Width of the floating composer.
const COMPOSER_WIDTH: f32 = 30.;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loose_times_parse_and_bad_ones_are_rejected() {
        let hm = |h, m| NaiveTime::from_hms_opt(h, m, 0);
        for (text, expected) in [
            ("9", hm(9, 0)),
            ("09:30", hm(9, 30)),
            ("930", hm(9, 30)),
            ("1415", hm(14, 15)),
            ("9.45", hm(9, 45)),
            ("2pm", hm(14, 0)),
            ("12am", hm(0, 0)),
            ("12:30 pm", hm(12, 30)),
            (" 7:05AM ", hm(7, 5)),
            ("24:00", None),
            ("13pm", None),
            ("9:75", None),
            ("noon", None),
            ("", None),
        ] {
            assert_eq!(parse_time(text), expected, "{text}");
        }
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration_label(Duration::minutes(45)), "45 min");
        assert_eq!(duration_label(Duration::minutes(60)), "1 h");
        assert_eq!(duration_label(Duration::minutes(90)), "1 h 30 min");
        assert_eq!(duration_label(Duration::days(2)), "2 d");
        assert_eq!(duration_label(Duration::minutes(-5)), "");
    }
}
