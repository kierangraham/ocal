use crate::{
    appearance::{action_button, reveal_field, tool_button},
    menus::{ContextTarget, menu},
    worker::{Command, Event as WorkerEvent, Worker},
};
use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Timelike, Utc};
use chrono_tz::Tz;
use gpui_kit::base::{
    ElementExt as _,
    input::{InputEvent, InputState, TextareaState},
};
use gpui_kit::{
    AnyElement, App, AppContext, Bounds, ClipboardItem, Context, Entity, FocusHandle, Focusable,
    FontWeight, IntoElement, KeyBinding, MouseButton, Pixels, Point, Render, ScrollHandle, Window,
    actions, div, point, prelude::*, px, relative, rems,
};
use gpui_omarchy::{
    ActiveTheme, ButtonVariant, ChoiceItem, ChoiceState, IconName, MenuItem, button, focus_scope,
    icon, input, select, textarea,
};
use ocal_core::{
    CalendarSet, Event, Repeat, Snapshot, local_datetime, month_days, move_month,
    selection::{Selection, Slot},
    system_zone, week_start, zone,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

actions!(
    ocal,
    [
        NewEvent,
        Today,
        Previous,
        Next,
        DayView,
        WeekView,
        MonthView,
        AgendaView,
        TasksView,
        Search,
        Save,
        Escape,
        Help,
        Sync,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        ToggleSidebar,
        ShowActions
    ]
);
pub fn bind_keys(cx: &mut App) {
    let normal = Some("Ocal && !Input && !Popover && !EventEditor");
    cx.bind_keys([
        KeyBinding::new("c", NewEvent, normal),
        KeyBinding::new("ctrl-n", NewEvent, Some("Ocal")),
        KeyBinding::new("t", Today, normal),
        KeyBinding::new("[", Previous, normal),
        KeyBinding::new("]", Next, normal),
        KeyBinding::new("1", DayView, normal),
        KeyBinding::new("2", WeekView, normal),
        KeyBinding::new("3", MonthView, normal),
        KeyBinding::new("4", AgendaView, normal),
        KeyBinding::new("5", TasksView, normal),
        KeyBinding::new("/", Search, normal),
        KeyBinding::new("ctrl-f", Search, Some("Ocal")),
        KeyBinding::new("ctrl-enter", Save, Some("Ocal")),
        KeyBinding::new("escape", Escape, Some("Ocal")),
        KeyBinding::new("?", Help, normal),
        KeyBinding::new("ctrl-r", Sync, Some("Ocal")),
        KeyBinding::new("b", ToggleSidebar, normal),
        KeyBinding::new("menu", ShowActions, normal),
        KeyBinding::new("shift-f10", ShowActions, normal),
        KeyBinding::new("ctrl-=", ZoomIn, Some("Ocal")),
        KeyBinding::new("ctrl--", ZoomOut, Some("Ocal")),
        KeyBinding::new("ctrl-0", ZoomReset, Some("Ocal")),
        KeyBinding::new(
            "enter",
            gpui_kit::base::actions::Confirm { secondary: false },
            normal,
        ),
    ]);
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum View {
    Day,
    Week,
    Month,
    Agenda,
    Tasks,
}
impl View {
    fn label(self) -> &'static str {
        match self {
            Self::Day => "Day",
            Self::Week => "Week",
            Self::Month => "Month",
            Self::Agenda => "Agenda",
            Self::Tasks => "Tasks",
        }
    }
}
const HOUR: f32 = 3.2;
/// Below this width the sidebar becomes an overlay. A half-width Hyprland tile on a
/// 1080p-class display is about 850 logical pixels, so it keeps the full week.
const NARROW: f32 = 920.;
/// Below this width the grid shows one day and the search moves under the heading.
const COMPACT: f32 = 640.;
struct Editor {
    base: Event,
    fields: Vec<Entity<InputState>>,
    notes: Entity<TextareaState>,
    calendar: Entity<ChoiceState>,
    repeat: Entity<ChoiceState>,
    all_day: bool,
    meeting: usize,
    meet_request: String,
    notify: bool,
    details: bool,
    editing: bool,
    discard: bool,
    initial: Vec<String>,
    focus: FocusHandle,
    last_focus: Option<usize>,
}

pub struct Ocal {
    snapshot: Snapshot,
    commands: mpsc::Sender<Command>,
    demo: bool,
    focus: FocusHandle,
    date: NaiveDate,
    view: View,
    selection: Selection,
    selected: Option<Event>,
    editor: Option<Editor>,
    search: Entity<InputState>,
    set_name: Entity<InputState>,
    status: String,
    error: bool,
    busy: bool,
    settings: bool,
    advanced_login: bool,
    sign_in_url: Option<String>,
    cancel_sign_in: Arc<AtomicBool>,
    help: bool,
    sidebar_open: bool,
    active_set: Option<String>,
    grid_scroll: ScrollHandle,
    form_scroll: ScrollHandle,
    agenda_scroll: ScrollHandle,
    grid_bounds: HashMap<NaiveDate, Bounds<Pixels>>,
    dragging: bool,
    reveal_cursor: bool,
    zoom: f32,
    event_menu: bool,
    slot_menu: bool,
    delete_confirm: bool,
    display_zone: String,
    restore_focus: bool,
    month_cell_height: Pixels,
    initial_grid_scroll: bool,
    slot_anchor: f32,
}
impl Ocal {
    pub fn new(worker: Worker, demo: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search events · /"));
        let set_name =
            cx.new(|cx| InputState::new(window, cx).placeholder("Name this calendar set"));
        cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify())
            .detach();
        let events = worker.events;
        cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if this.update(cx, |this, cx| this.event(event, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        // Keep the now-line and today's highlight current while the window stays open.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(30))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        let display_zone = system_zone();
        let date = Utc::now().with_timezone(&zone(&display_zone)).date_naive();
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            snapshot: Snapshot::default(),
            commands: worker.commands,
            demo,
            focus,
            date,
            view: View::Week,
            selection: Selection::new(date),
            selected: None,
            editor: None,
            search,
            set_name,
            status: if demo {
                "Demo calendar · changes stay in this session".into()
            } else {
                "Your calendar, at your pace".into()
            },
            error: false,
            busy: false,
            settings: false,
            advanced_login: false,
            sign_in_url: None,
            cancel_sign_in: worker.cancel_sign_in,
            help: false,
            sidebar_open: false,
            active_set: None,
            grid_scroll: ScrollHandle::new(),
            form_scroll: ScrollHandle::new(),
            agenda_scroll: ScrollHandle::new(),
            grid_bounds: HashMap::new(),
            dragging: false,
            reveal_cursor: true,
            zoom: 1.,
            event_menu: false,
            slot_menu: false,
            delete_confirm: false,
            display_zone,
            restore_focus: false,
            month_cell_height: px(100.),
            initial_grid_scroll: true,
            slot_anchor: 0.5,
        }
    }
    fn event(&mut self, event: WorkerEvent, cx: &mut Context<Self>) {
        match event {
            WorkerEvent::Snapshot(s) => {
                self.snapshot = s;
                if self.status == "Saved · Generating Google Meet…"
                    && let Some(selected) = &self.selected
                    && let Some(updated) = self.snapshot.events.iter().find(|e| e.id == selected.id)
                    && updated.conference_status != "pending"
                {
                    self.status = if !updated.link.is_empty() {
                        "Saved · Google Meet is ready"
                    } else {
                        "Google could not generate a meeting link; edit the event to retry"
                    }
                    .into();
                }
                if let Some(e) = &self.selected {
                    if let Some(updated) = self.snapshot.events.iter().find(|x| x.id == e.id) {
                        if updated.repeat == Repeat::Never {
                            self.selected = Some(updated.clone());
                        }
                    } else {
                        self.selected = None;
                    }
                }
            }
            WorkerEvent::SignInReady(url) => {
                cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
                self.sign_in_url = Some(url);
                self.busy = true;
                self.error = false;
                self.status =
                    "Sign-in link copied · Paste it into your preferred browser profile".into();
            }
            WorkerEvent::Progress(s) => {
                self.sign_in_url = None;
                self.status = s;
                self.error = false;
            }
            WorkerEvent::Status(s) => {
                self.sign_in_url = None;
                self.status = s;
                self.error = false;
                self.busy = false;
            }
            WorkerEvent::Error(s) => {
                self.sign_in_url = None;
                self.status = s;
                self.error = true;
                self.busy = false;
                if let Some(f) = &self.editor {
                    for field in &f.fields {
                        field.update(cx, |s, cx| s.set_disabled(false, cx));
                    }
                    f.notes.update(cx, |s, cx| s.set_disabled(false, cx));
                }
            }
            WorkerEvent::Saved(saved) => {
                self.restore_focus = true;
                self.editor = None;
                self.selected = Some(*saved);
                self.busy = false;
                self.status = if self
                    .selected
                    .as_ref()
                    .is_some_and(|e| e.conference_status == "pending")
                {
                    "Saved · Generating Google Meet…".into()
                } else {
                    "Saved".into()
                };
                self.error = false;
            }
            WorkerEvent::Connected => {
                self.sign_in_url = None;
                self.settings = false;
                self.busy = false;
                self.error = false;
                self.active_set = None;
                self.restore_focus = true;
                self.status = "Google Calendar connected".into();
            }
            WorkerEvent::Deleted => {
                self.restore_focus = true;
                self.selected = None;
                self.delete_confirm = false;
                self.busy = false;
                self.status = "Event deleted".into();
                self.error = false;
            }
        }
        cx.notify();
    }
    fn dispatch(&self, command: Command) {
        let _ = self.commands.send(command);
    }
    fn tz(&self) -> Tz {
        zone(&self.display_zone)
    }
    fn visible(&self, calendar_id: &str) -> bool {
        self.snapshot
            .calendars
            .iter()
            .any(|c| c.id == calendar_id && c.visible)
            && self.active_set.as_ref().is_none_or(|id| {
                self.snapshot
                    .sets
                    .iter()
                    .find(|s| &s.id == id)
                    .is_none_or(|s| s.calendars.iter().any(|c| c == calendar_id))
            })
    }
    fn calendar_color(&self, id: &str, cx: &App) -> gpui_kit::Hsla {
        let t = cx.omarchy();
        self.snapshot
            .calendars
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| u32::from_str_radix(c.color.trim_start_matches('#'), 16).ok())
            .map(|v| gpui_kit::rgb(v).into())
            .unwrap_or(t.accent)
    }
    /// Calendar IDs shown by the sidebar checkboxes and the active calendar set.
    fn visible_calendars(&self) -> HashSet<&str> {
        self.snapshot
            .calendars
            .iter()
            .filter(|c| self.visible(&c.id))
            .map(|c| c.id.as_str())
            .collect()
    }
    fn events(&self, from: NaiveDate, to: NaiveDate, cx: &App) -> Vec<Event> {
        let query = self.search.read(cx).value().trim().to_lowercase();
        let visible = self.visible_calendars();
        let tz = self.tz();
        let matches = |text: &str| text.to_lowercase().contains(&query);
        let mut events: Vec<_> = self
            .snapshot
            .events
            .iter()
            .filter(|e| !e.completed && visible.contains(e.calendar.as_str()))
            .filter(|e| {
                query.is_empty() || matches(&e.title) || matches(&e.location) || matches(&e.notes)
            })
            .flat_map(|e| {
                ocal_core::layout::occurrences(e, from - Duration::days(2), to + Duration::days(2))
            })
            .filter(|e| {
                let (s, t) = e.dates_in(tz);
                s < to && t > from
            })
            .collect();
        events.sort_by_key(|e| e.start);
        events
    }
    fn writable(&self, e: &Event) -> bool {
        self.snapshot
            .calendars
            .iter()
            .any(|c| c.id == e.calendar && c.writable)
    }
    fn event_at_cursor(&self, cx: &App) -> Option<Event> {
        if !matches!(self.view, View::Day | View::Week) || self.selection.anchor.is_some() {
            return None;
        }
        let cursor = local_datetime(self.selection.cursor.datetime(), &self.display_zone).ok()?;
        self.events(
            self.selection.cursor.day,
            self.selection.cursor.day + Duration::days(1),
            cx,
        )
        .into_iter()
        .filter(|e| e.all_day.is_none() && !e.task && e.start <= cursor && cursor < e.end)
        // Prefer the newly crossed event when appointments overlap.
        .min_by_key(|e| (std::cmp::Reverse(e.start), e.end, e.id.clone()))
    }
    fn choose_date(&mut self, date: NaiveDate, cx: &mut Context<Self>) {
        let sync = date.month() != self.date.month() || date.year() != self.date.year();
        self.date = date;
        self.selection.cursor.day = date;
        self.selection.anchor = None;
        self.selected = None;
        self.reveal_cursor = true;
        if sync {
            self.dispatch(Command::Sync(date));
        }
        cx.notify();
    }
    fn navigate(&mut self, n: i32, cx: &mut Context<Self>) {
        let date = match self.view {
            View::Day => self.date + Duration::days(n as i64),
            View::Week | View::Agenda | View::Tasks => self.date + Duration::days(7 * n as i64),
            View::Month => move_month(self.date, n),
        };
        self.choose_date(date, cx);
    }
    fn change_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.is_some() {
            return;
        }
        self.view = view;
        self.selected = None;
        window.focus(&self.focus, cx);
        self.reveal_cursor = true;
        cx.notify();
    }
    fn new_event(&mut self, task: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.editor.is_some() {
            return;
        }
        let chosen = self
            .snapshot
            .calendars
            .iter()
            .filter(|c| c.writable && self.visible(&c.id) && (!task || c.account.is_none()))
            .min_by_key(|c| {
                if task {
                    usize::from(c.id != "local-tasks")
                } else {
                    usize::from(c.account.is_none())
                }
            });
        let Some(calendar) = chosen.or_else(|| {
            self.snapshot
                .calendars
                .iter()
                .find(|c| c.writable && (!task || c.account.is_none()))
        }) else {
            self.status = "Connect a calendar or add a local calendar first".into();
            self.error = true;
            cx.notify();
            return;
        };
        let mut e = Event::new(&calendar.id, self.date, &self.display_zone).unwrap();
        let (s, t) = self.selection.range();
        match (
            local_datetime(s, &self.display_zone),
            local_datetime(t, &self.display_zone),
        ) {
            (Ok(s), Ok(t)) => {
                e.start = s;
                e.end = t;
            }
            _ => {
                self.status = "Choose a time outside the daylight-saving transition".into();
                self.error = true;
                cx.notify();
                return;
            }
        }
        e.task = task;
        e.busy = !task;
        if self.view == View::Month {
            e.all_day = Some((self.date, self.date + Duration::days(1)));
        }
        self.open_editor(e, false, window, cx);
    }
    fn open_editor(
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
        let values = vec![
            e.title.clone(),
            s.to_string(),
            start.format("%H:%M").to_string(),
            t.to_string(),
            end.format("%H:%M").to_string(),
            e.location.clone(),
            e.link.clone(),
            e.attendees
                .iter()
                .map(|a| a.email.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            e.timezone.clone(),
        ];
        let fields: Vec<_> = values
            .iter()
            .enumerate()
            .map(|(i, value)| {
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .default_value(value.clone())
                        .placeholder(match i {
                            0 => "Event title",
                            5 => "Add a location",
                            6 => "https://…",
                            7 => "Guests’ email addresses",
                            _ => "",
                        })
                })
            })
            .collect();
        for field in &fields {
            cx.subscribe(field, |_, _, _: &InputEvent, cx| cx.notify())
                .detach();
        }
        let calendars: Vec<_> = self
            .snapshot
            .calendars
            .iter()
            .filter(|c| {
                c.writable
                    && (!e.task || c.account.is_none())
                    && (e.remote_id.is_none() || c.id == e.calendar)
            })
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
        let repeats = [
            Repeat::Never,
            Repeat::Daily,
            Repeat::Weekly,
            Repeat::Monthly,
            Repeat::Yearly,
        ];
        let repeat = cx.new(|cx| {
            ChoiceState::new(
                repeats
                    .iter()
                    .enumerate()
                    .map(|(i, r)| ChoiceItem::new(i.to_string(), r.label()))
                    .collect(),
                window,
                cx,
            )
            .label("Repeat")
            .default_selected(repeats.iter().position(|r| *r == e.repeat).unwrap_or(0))
        });
        cx.observe(&calendar, |_, _, cx| cx.notify()).detach();
        cx.observe(&repeat, |_, _, cx| cx.notify()).detach();
        let notes = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(3)
                .default_value(e.notes.clone())
                .placeholder("Add notes")
        });
        cx.subscribe(&notes, |_, _, _: &InputEvent, cx| cx.notify())
            .detach();
        fields[0].update(cx, |s, cx| s.focus(window, cx));
        let all_day = e.all_day.is_some();
        let meeting = usize::from(!e.link.is_empty());
        self.form_scroll.set_offset(point(px(0.), px(0.)));
        self.editor = Some(Editor {
            base: e,
            fields,
            notes,
            calendar,
            repeat,
            all_day,
            meeting,
            meet_request: uuid::Uuid::new_v4().simple().to_string(),
            notify: true,
            details: false,
            editing,
            discard: false,
            initial: values,
            focus: cx.focus_handle(),
            last_focus: None,
        });
        self.selected = None;
        self.settings = false;
        self.help = false;
        self.sidebar_open = false;
        self.error = false;
        cx.notify();
    }
    fn draft(&self, cx: &App) -> anyhow::Result<Event> {
        let f = self
            .editor
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No event editor"))?;
        let value = |i: usize| f.fields[i].read(cx).value().trim().to_owned();
        let mut e = f.base.clone();
        e.title = value(0);
        e.calendar = f
            .calendar
            .read(cx)
            .selected()
            .ok_or_else(|| anyhow::anyhow!("Choose a writable calendar"))?
            .value
            .to_string();
        e.timezone = value(8);
        let s = NaiveDate::parse_from_str(&value(1), "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("Use a start date like 2026-10-05"))?;
        let t = NaiveDate::parse_from_str(&value(3), "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("Use an end date like 2026-10-05"))?;
        if f.all_day {
            e.all_day = Some((s, t + Duration::days(1)));
            e.start = s.and_hms_opt(0, 0, 0).unwrap().and_utc();
            e.end = (t + Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc();
        } else {
            e.all_day = None;
            let start = NaiveTime::parse_from_str(&value(2), "%H:%M")
                .map_err(|_| anyhow::anyhow!("Use a start time like 09:15"))?;
            let end = NaiveTime::parse_from_str(&value(4), "%H:%M")
                .map_err(|_| anyhow::anyhow!("Use an end time like 10:00"))?;
            e.start = local_datetime(s.and_time(start), &e.timezone)?;
            e.end = local_datetime(t.and_time(end), &e.timezone)?;
        }
        e.location = value(5);
        e.notes = f.notes.read(cx).value().to_string();
        e.link = if f.meeting == 1 {
            value(6)
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
        e.attendees = value(7)
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
            e.repeat = [
                Repeat::Never,
                Repeat::Daily,
                Repeat::Weekly,
                Repeat::Monthly,
                Repeat::Yearly,
            ][f
                .repeat
                .read(cx)
                .selected()
                .and_then(|s| s.value.parse::<usize>().ok())
                .unwrap_or(0)];
        }
        e.validate()?;
        Ok(e)
    }
    fn save(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        match self.draft(cx) {
            Ok(event) => {
                let notify = self.editor.as_ref().is_some_and(|f| f.notify);
                self.busy = true;
                if let Some(f) = &self.editor {
                    for field in &f.fields {
                        field.update(cx, |s, cx| s.set_disabled(true, cx));
                    }
                    f.notes.update(cx, |s, cx| s.set_disabled(true, cx));
                }
                self.error = false;
                self.status = "Saving…".into();
                self.dispatch(Command::Save { event, notify });
            }
            Err(e) => {
                self.status = format!("{e:#}");
                self.error = true;
            }
        }
        cx.notify();
    }
    fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Some(f) = &mut self.editor {
            let dirty = f
                .fields
                .iter()
                .enumerate()
                .any(|(i, s)| s.read(cx).value().as_ref() != f.initial[i])
                || f.notes.read(cx).value().as_ref() != f.base.notes
                || f.all_day != f.base.all_day.is_some()
                || f.meeting != usize::from(!f.base.link.is_empty())
                || f.repeat
                    .read(cx)
                    .selected()
                    .and_then(|s| s.value.parse::<usize>().ok())
                    .unwrap_or(0)
                    != [
                        Repeat::Never,
                        Repeat::Daily,
                        Repeat::Weekly,
                        Repeat::Monthly,
                        Repeat::Yearly,
                    ]
                    .iter()
                    .position(|r| *r == f.base.repeat)
                    .unwrap_or(0)
                || f.calendar
                    .read(cx)
                    .selected()
                    .is_some_and(|s| s.value.as_ref() != f.base.calendar);
            if dirty {
                f.discard = true;
                cx.notify();
                return;
            }
        }
        self.editor = None;
        self.selection.anchor = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.is_some() {
            self.close_editor(window, cx);
            return;
        }
        if self.selected.is_some() {
            self.selected = None;
        } else if self.settings || self.help || self.sidebar_open {
            self.settings = false;
            self.help = false;
            self.sidebar_open = false;
        } else if self.selection.anchor.is_some() {
            self.selection.anchor = None;
        } else {
            self.search.update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.dragging = false;
        window.focus(&self.focus, cx);
        cx.notify();
    }
    fn pick_event(&mut self, e: Event, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.is_some() {
            return;
        }
        self.selected = Some(e);
        self.delete_confirm = false;
        window.focus(&self.focus, cx);
        cx.notify();
    }
    fn duplicate(&mut self, e: &Event, window: &mut Window, cx: &mut Context<Self>) {
        let mut e = e.clone();
        e.id = uuid::Uuid::new_v4().simple().to_string();
        e.remote_id = None;
        e.etag = None;
        e.series_id = None;
        e.repeat = Repeat::Never;
        e.attendees.clear();
        e.meet_request = None;
        e.has_conference = false;
        e.clear_conference = false;
        e.conference_status.clear();
        if !e.link.is_empty() {
            e.notes = e
                .notes
                .lines()
                .filter(|line| line.trim() != e.link)
                .collect::<Vec<_>>()
                .join("\n")
                .trim_end()
                .to_owned();
        }
        e.link.clear();
        e.title = format!("{} (copy)", e.title);
        self.open_editor(e, false, window, cx);
    }
    fn event_action(
        &mut self,
        e: Event,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match index {
            0 => self.pick_event(e, window, cx),
            1 => {
                if self.writable(&e) {
                    self.open_editor(e, true, window, cx)
                }
            }
            2 => self.duplicate(&e, window, cx),
            3 => {
                self.pick_event(e, window, cx);
                self.delete_confirm = true;
                cx.notify();
            }
            _ => (),
        }
    }
    fn event_items(&self, e: &Event) -> Vec<MenuItem> {
        vec![
            MenuItem::new("Details").icon(IconName::CalendarDays),
            MenuItem::new(if e.series_id.is_some() && e.remote_id.is_none() {
                "Edit series"
            } else {
                "Edit event"
            })
            .icon(IconName::PencilLine)
            .disabled(!self.writable(e)),
            MenuItem::new("Duplicate").icon(IconName::Copy),
            MenuItem::new(if e.series_id.is_some() && e.remote_id.is_none() {
                "Delete series…"
            } else {
                "Delete event…"
            })
            .icon(IconName::Trash)
            .separator_before()
            .disabled(!self.writable(e)),
        ]
    }
    fn mini_month(
        &self,
        date: NaiveDate,
        compact: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        let cell = if compact { 1.6 } else { 1.85 };
        let mut grid = div().flex().flex_col().gap(rems(0.12));
        let today = Utc::now().with_timezone(&self.tz()).date_naive();
        for (week, days) in month_days(date).chunks(7).enumerate() {
            grid = grid.child(div().flex().children(days.iter().enumerate().map(|(i, d)| {
                let d = *d;
                let selected = d == self.date;
                let today = d == today;
                let mut b = button(
                    format!("mini-{}-{week}-{i}", date.format("%Y%m")),
                    d.day().to_string(),
                    ButtonVariant::Secondary,
                    cx,
                )
                .accessibility_label(d.format("%A %d %B %Y").to_string())
                .w(rems(cell))
                .h(rems(cell))
                .p(rems(0.))
                .border_0()
                .text_size(rems(0.7))
                .text_color(if today {
                    t.accent
                } else if d.month() == date.month() {
                    t.foreground
                } else {
                    t.secondary.opacity(0.5)
                })
                .bg(if selected {
                    t.selection
                } else {
                    t.foreground.opacity(0.)
                });
                if today {
                    b = b.font_weight(FontWeight::SEMIBOLD);
                }
                b.on_click(cx.listener(move |app, _, _, cx| app.choose_date(d, cx)))
            })));
        }
        let _ = window;
        div()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pb(rems(0.6))
                    .child(
                        div()
                            .text_size(rems(0.86))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(date.format("%B %Y").to_string()),
                    )
                    .when(!compact, |row| {
                        row.child(
                            tool_button(
                                "mini-prev",
                                IconName::ChevronLeft,
                                "Previous month",
                                "",
                                cx,
                            )
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.choose_date(move_month(app.date, -1), cx)
                            })),
                        )
                        .child(
                            tool_button("mini-next", IconName::ChevronRight, "Next month", "", cx)
                                .on_click(cx.listener(|app, _, _, cx| {
                                    app.choose_date(move_month(app.date, 1), cx)
                                })),
                        )
                    }),
            )
            .child(div().flex().pb(rems(0.3)).children(
                ["M", "T", "W", "T", "F", "S", "S"].into_iter().map(|s| {
                    div()
                        .w(rems(cell))
                        .text_center()
                        .text_size(rems(0.63))
                        .text_color(t.secondary)
                        .child(s)
                }),
            ))
            .child(grid)
            .into_any_element()
    }
    fn calendar_sets(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = cx.entity().downgrade();
        let sets = self.snapshot.sets.clone();
        let active = self.active_set.clone();
        let mut items = vec![MenuItem::new("All calendars").checked(active.is_none())];
        items.extend(
            sets.iter()
                .map(|s| MenuItem::new(s.name.clone()).checked(active.as_ref() == Some(&s.id))),
        );
        menu(
            "calendar-sets",
            button("calendar-set-trigger", "", ButtonVariant::Secondary, cx)
                .border_0()
                .child(
                    self.active_set
                        .as_ref()
                        .and_then(|id| self.snapshot.sets.iter().find(|s| &s.id == id))
                        .map(|s| s.name.clone())
                        .unwrap_or_else(|| "All calendars".into()),
                )
                .child(icon(IconName::ChevronDown).size(rems(0.8))),
            items,
            move |index, window, cx| {
                let _ = target.update(cx, |app, cx| {
                    app.active_set = if index == 0 {
                        None
                    } else {
                        sets.get(index - 1).map(|s| s.id.clone())
                    };
                    app.selected = None;
                    window.focus(&app.focus, cx);
                    cx.notify();
                });
            },
        )
        .into_any_element()
    }
    fn sidebar(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let mut side = div()
            .w(rems(16.5))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(t.inset)
            .border_r_1()
            .border_color(t.divider())
            .child(
                div()
                    .p(rems(1.1))
                    .child(self.mini_month(self.date, false, window, cx)),
            )
            .child(
                div()
                    .px(rems(1.))
                    .pb(rems(0.7))
                    .child(self.calendar_sets(cx)),
            );
        let mut calendar_list = div()
            .id("sidebar-calendars")
            .min_h_0()
            .max_h(rems(18.))
            .overflow_y_scroll();
        for (group_index, (account, indices)) in
            calendar_groups(&self.snapshot).into_iter().enumerate()
        {
            calendar_list = calendar_list.child(
                div()
                    .debug_selector(move || format!("calendar-group-{group_index}"))
                    .px(rems(1.15))
                    .pt(rems(0.65))
                    .pb(rems(0.3))
                    .text_size(rems(0.63))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.secondary)
                    .text_ellipsis()
                    .child(account.unwrap_or_else(|| "Local calendars".into())),
            );
            for i in indices {
                let c = self.snapshot.calendars[i].clone();
                let visible = self.visible(&c.id);
                let color = self.calendar_color(&c.id, cx);
                calendar_list = calendar_list.child(
                    button(("calendar-row", i), "", ButtonVariant::Secondary, cx)
                        .debug_selector(move || format!("calendar-row-{i}"))
                        .accessibility_label(format!(
                            "{} calendar, {}",
                            c.display_name(),
                            if visible { "visible" } else { "hidden" }
                        ))
                        .w_full()
                        .border_0()
                        .px(rems(1.15))
                        .py(rems(0.4))
                        .gap(rems(0.65))
                        .justify_start()
                        .child(
                            icon(if visible {
                                IconName::SquareCheck
                            } else {
                                IconName::Square
                            })
                            .size(rems(0.9))
                            .text_color(color),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .text_size(rems(0.76))
                                .child(c.name.clone()),
                        )
                        .on_click(cx.listener(move |app, _, window, cx| {
                            window.focus(&app.focus, cx);
                            let mut c = c.clone();
                            c.visible = !c.visible;
                            if let Some(current) =
                                app.snapshot.calendars.iter_mut().find(|x| x.id == c.id)
                            {
                                current.visible = c.visible;
                            }
                            app.active_set = None;
                            app.dispatch(Command::Visible(c));
                            cx.notify();
                        })),
                );
            }
        }
        side = side.child(calendar_list).child(
            action_button(
                "sidebar-accounts",
                IconName::Plus,
                "Add Google account",
                ButtonVariant::Secondary,
                cx,
            )
            .mx(rems(0.8))
            .mt(rems(0.4))
            .border_0()
            .debug_selector(|| "sidebar-accounts".into())
            .on_click(cx.listener(|app, _, _, cx| {
                app.settings = true;
                app.help = false;
                app.selected = None;
                app.sidebar_open = false;
                cx.notify();
            })),
        );
        let upcoming = self.events(self.date, self.date + Duration::days(5), cx);
        let mut agenda = div()
            .id("sidebar-agenda")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(rems(1.))
            .pt(rems(1.))
            .child(
                div()
                    .pb(rems(0.65))
                    .text_color(t.secondary)
                    .text_size(rems(0.62))
                    .child("UP NEXT"),
            );
        if upcoming.is_empty() {
            agenda = agenda.child(
                div()
                    .text_color(t.secondary)
                    .text_size(rems(0.74))
                    .child("A little breathing room."),
            );
        }
        let mut last = None;
        for (i, e) in upcoming.iter().take(15).enumerate() {
            let day = e.dates_in(self.tz()).0;
            if last != Some(day) {
                agenda = agenda.child(
                    div()
                        .pt(rems(0.6))
                        .pb(rems(0.35))
                        .text_size(rems(0.7))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(day.format("%a, %-d %b").to_string()),
                );
                last = Some(day);
            }
            let e = e.clone();
            let color = self.calendar_color(&e.calendar, cx);
            agenda =
                agenda.child(
                    button(("upcoming", i), "", ButtonVariant::Secondary, cx)
                        .border_0()
                        .w_full()
                        .justify_start()
                        .px(rems(0.3))
                        .py(rems(0.4))
                        .gap(rems(0.55))
                        .child(div().w(px(2.)).h(rems(1.8)).bg(color))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(rems(0.15))
                                .child(
                                    div()
                                        .text_size(rems(0.73))
                                        .text_ellipsis()
                                        .child(e.title.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(rems(0.63))
                                        .text_color(t.secondary)
                                        .child(e.time_label(self.tz())),
                                ),
                        )
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.pick_event(e.clone(), window, cx)
                        })),
                );
        }
        side = side.child(agenda).child(
            div()
                .border_t_1()
                .border_color(t.divider())
                .p(rems(0.65))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    action_button(
                        "settings",
                        IconName::Settings,
                        "Settings",
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .border_0()
                    .on_click(cx.listener(|app, _, window, cx| {
                        if app.editor.is_none() {
                            app.settings = true;
                            app.help = false;
                            app.selected = None;
                            window.focus(&app.focus, cx);
                            cx.notify();
                        }
                    })),
                )
                .child(
                    tool_button(
                        "help",
                        IconName::CircleQuestionMark,
                        "Keyboard shortcuts",
                        "?",
                        cx,
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.help = true;
                        cx.notify();
                    })),
                ),
        );
        side.into_any_element()
    }
    fn event_chip(
        &self,
        e: &Event,
        index: usize,
        small: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        let color = self.calendar_color(&e.calendar, cx);
        let e = e.clone();
        let pick = e.clone();
        let action = e.clone();
        let selected = self
            .selected
            .as_ref()
            .is_some_and(|s| s.id == e.id && s.start == e.start);
        let target = cx.entity().downgrade();
        let button = button(
            (if small { "month-event" } else { "agenda-event" }, index),
            "",
            ButtonVariant::Secondary,
            cx,
        )
        .debug_selector(move || format!("event-{index}"))
        .accessibility_label(format!("{}, {}", e.title, e.time_label(self.tz())))
        .w_full()
        .min_w_0()
        .border_0()
        .border_l_2()
        .border_color(color)
        .bg(if selected {
            t.selection
        } else {
            color.opacity(0.09)
        })
        .hover(move |s| s.bg(color.opacity(0.17)))
        .px(rems(0.4))
        .py(rems(if small { 0.18 } else { 0.6 }))
        .justify_start()
        .gap(rems(0.5))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_ellipsis()
                .text_size(rems(if small { 0.65 } else { 0.84 }))
                .child(e.title.clone()),
        )
        .children((!small).then(|| {
            div()
                .flex_shrink_0()
                .text_size(rems(0.7))
                .text_color(t.secondary)
                .child(e.time_label(self.tz()))
        }))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |app, _, window, cx| app.pick_event(pick.clone(), window, cx)));
        menu(
            ("event-menu", index),
            button,
            self.event_items(&e),
            move |index, window, cx| {
                let _ = target.update(cx, |app, cx| {
                    app.event_action(action.clone(), index, window, cx)
                });
            },
        )
        .mouse_button(MouseButton::Right)
        .into_any_element()
    }
    fn pointer_slot(&self, p: Point<Pixels>) -> Option<Slot> {
        self.grid_bounds.iter().find_map(|(date, b)| {
            if p.x >= b.left() && p.x <= b.right() && p.y >= b.top() && p.y <= b.bottom() {
                let fraction = (p.y - b.top()) / b.size.height;
                Some(Slot::new(*date, (fraction * 1440.) as u16))
            } else {
                None
            }
        })
    }
    fn grid(
        &mut self,
        cursor_event: Option<&Event>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        let compact = window.viewport_size().width < px(COMPACT * self.zoom);
        // Dates from a previous period would otherwise still claim pointer hits.
        self.grid_bounds.clear();
        let days = if self.view == View::Day || compact {
            vec![self.date]
        } else {
            let first = week_start(self.date);
            (0..7)
                .map(|i| first + Duration::days(i))
                .collect::<Vec<_>>()
        };
        let events = self.events(days[0], *days.last().unwrap() + Duration::days(1), cx);
        let today = Utc::now().with_timezone(&self.tz()).date_naive();
        let target = cx.entity().downgrade();
        let mut header = div()
            .flex()
            .pl(rems(3.4))
            .border_b_1()
            .border_color(t.divider());
        for (i, date) in days.iter().enumerate() {
            let date = *date;
            header = header.child(
                button(("day-heading", i), "", ButtonVariant::Secondary, cx)
                    .flex_1()
                    .min_w_0()
                    .border_0()
                    .py(rems(0.7))
                    .flex_col()
                    .gap(rems(0.25))
                    .bg(if date == self.date {
                        t.accent.opacity(0.035)
                    } else {
                        t.foreground.opacity(0.)
                    })
                    .child(
                        div()
                            .text_size(rems(0.65))
                            .text_color(t.secondary)
                            .child(date.format("%a").to_string()),
                    )
                    .child(day_number(
                        date.day(),
                        date == today,
                        t.bright,
                        rems(1.1),
                        rems(2.),
                        &t,
                    ))
                    .on_click(cx.listener(move |app, _, _, cx| app.choose_date(date, cx))),
            );
        }
        let mut all_day = div()
            .flex()
            .min_h(rems(2.))
            .border_b_1()
            .border_color(t.divider())
            .child(
                div()
                    .w(rems(3.4))
                    .flex_shrink_0()
                    .p(rems(0.35))
                    .text_size(rems(0.58))
                    .text_color(t.secondary)
                    .child("all-day"),
            );
        let mut counter = 30000;
        for date in &days {
            let mut day = div()
                .flex_1()
                .min_w_0()
                .px(rems(0.15))
                .py(rems(0.25))
                .flex()
                .flex_col()
                .gap(rems(0.12));
            for e in events
                .iter()
                .filter(|e| e.all_day.is_some() && !e.task && e.on_date(*date, self.tz()))
            {
                day = day.child(self.event_chip(e, counter, true, cx));
                counter += 1;
            }
            all_day = all_day.child(day);
        }
        let mut hours = div()
            .w(rems(3.4))
            .flex_shrink_0()
            .h(rems(HOUR * 24.))
            .flex()
            .flex_col();
        for h in 0..24 {
            // Centre each label on its hour line, as the line is what times refer to.
            hours = hours.child(
                div()
                    .h(rems(HOUR))
                    .relative()
                    .top(rems(-0.4))
                    .text_size(rems(0.62))
                    .text_color(t.secondary)
                    .pr(rems(0.5))
                    .text_right()
                    .when(h > 0, |d| d.child(format!("{h:02}:00"))),
            );
        }
        // One shared layer of hour and half-hour lines beneath every column.
        let lines = div()
            .absolute()
            .top_0()
            .left(rems(3.4))
            .right_0()
            .h(rems(HOUR * 24.))
            .children((1..48).map(|half| {
                div()
                    .absolute()
                    .top(rems(half as f32 * HOUR / 2.))
                    .left_0()
                    .right_0()
                    .h(px(1.))
                    .bg(t.divider().opacity(if half % 2 == 0 { 0.65 } else { 0.22 }))
            }));
        let mut grid = div().w_full().relative().flex().child(lines).child(hours);
        let reveal = self.reveal_cursor;
        self.reveal_cursor = false;
        for (di, date) in days.iter().enumerate() {
            let date = *date;
            let bounds_target = target.clone();
            let mut col = div()
                .id(("grid-day", di))
                .debug_selector(move || format!("grid-day-{di}"))
                .w_full()
                .min_w_0()
                .h(rems(HOUR * 24.))
                .relative()
                .border_l_1()
                .border_color(t.divider())
                .when(date == today, |col| col.bg(t.accent.opacity(0.035)))
                .on_prepaint(move |bounds, _, cx| {
                    let _ = bounds_target.update(cx, |app, _| {
                        app.grid_bounds.insert(date, bounds);
                    });
                });
            let tz = self.tz();
            let timed: Vec<_> = events
                .iter()
                .filter(|e| e.all_day.is_none() && !e.task && e.on_date(date, tz))
                .cloned()
                .collect();
            let intervals: Vec<_> = timed
                .iter()
                .map(|e| {
                    let s = e.start.with_timezone(&tz);
                    let end = e.end.with_timezone(&tz);
                    let start = if s.date_naive() < date {
                        0.
                    } else {
                        s.num_seconds_from_midnight() as f32 / 3600.
                    };
                    let end = if end.date_naive() > date {
                        24.
                    } else {
                        end.num_seconds_from_midnight() as f32 / 3600.
                    };
                    (start, end)
                })
                .collect();
            // The cursor sits beneath events; a crossed event is highlighted instead.
            if date == self.selection.cursor.day {
                let cursor = div()
                    .debug_selector(|| "time-cursor".into())
                    .absolute()
                    .left(px(2.))
                    .right(px(2.))
                    .top(rems(self.selection.cursor.minute as f32 / 60. * HOUR))
                    .h(px(2.))
                    .bg(t.accent)
                    .child(
                        div()
                            .absolute()
                            .left(-px(2.))
                            .top(-px(2.))
                            .size(px(6.))
                            .rounded_full()
                            .bg(t.accent),
                    );
                let scroll = self.grid_scroll.clone();
                col = col.child(cursor.on_prepaint(move |bounds, window, _| {
                    if !reveal {
                        return;
                    }
                    let viewport = scroll.bounds();
                    let delta = if bounds.top() < viewport.top() {
                        viewport.top() - bounds.top()
                    } else if bounds.bottom() > viewport.bottom() {
                        viewport.bottom() - bounds.bottom()
                    } else {
                        px(0.)
                    };
                    if delta != px(0.) {
                        let next =
                            (scroll.offset().y + delta).clamp(-scroll.max_offset().y, px(0.));
                        scroll.set_offset(point(px(0.), next));
                        window.refresh();
                    }
                }));
            }
            for placement in ocal_core::layout::lanes(&intervals) {
                let e = timed[placement.index].clone();
                let color = self.calendar_color(&e.calendar, cx);
                let ev = e.clone();
                let target = cx.entity().downgrade();
                let index = di * 1000 + placement.index;
                let duration = placement.end - placement.start;
                let compact_event = duration < 0.75;
                let highlighted = self
                    .selected
                    .as_ref()
                    .or(cursor_event)
                    .is_some_and(|s| s.id == e.id && s.start == e.start);
                let trigger = button(("grid-event", index), "", ButtonVariant::Secondary, cx)
                    .debug_selector(move || format!("grid-event-{index}"))
                    .accessibility_label(format!("{}, {}", e.title, e.time_label(tz)))
                    // Popover adds an auto-height wrapper: a percentage height
                    // resolves to content size there, rather than event duration.
                    .w_full()
                    .h(rems(duration * HOUR))
                    .min_h_0()
                    .flex_shrink_0()
                    .border_0()
                    .when(highlighted, |s| s.border_1())
                    .border_l_2()
                    .border_color(if highlighted { t.accent } else { color })
                    // Opaque fills keep grid lines and the time cursor out of titles.
                    .bg(tint(&t, color, if highlighted { 0.3 } else { 0.16 }))
                    .hover({
                        let hover = tint(&t, color, if highlighted { 0.34 } else { 0.26 });
                        move |s| s.bg(hover)
                    })
                    .px(rems(0.35))
                    .py(rems(if compact_event { 0.04 } else { 0.2 }))
                    .justify_start()
                    .items_start()
                    .overflow_hidden()
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(rems(0.16))
                            .children(highlighted.then(|| {
                                div()
                                    .debug_selector(move || format!("highlighted-event-{index}"))
                                    .absolute()
                                    .size(px(1.))
                            }))
                            .child(
                                div()
                                    .text_size(rems(if compact_event { 0.64 } else { 0.7 }))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(event_text(&t, color))
                                    .when(duration >= 1.5, |s| s.whitespace_normal())
                                    .line_clamp(if duration >= 1.5 { 2 } else { 1 })
                                    .text_ellipsis()
                                    .child(e.title.clone()),
                            )
                            .children((!compact_event).then(|| {
                                div()
                                    .text_ellipsis()
                                    .text_size(rems(0.61))
                                    .text_color(t.secondary)
                                    .child(e.time_label(tz))
                            }))
                            .children((!e.location.is_empty() && duration >= 1.5).then(|| {
                                div()
                                    .text_ellipsis()
                                    .text_size(rems(0.6))
                                    .text_color(t.secondary)
                                    .child(e.location.clone())
                            })),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.pick_event(ev.clone(), window, cx)
                    }));
                let action = e.clone();
                col = col.child(
                    div()
                        .absolute()
                        .top(rems(placement.start * HOUR))
                        .h(rems((placement.end - placement.start) * HOUR))
                        .left(relative(placement.column as f32 / placement.columns as f32))
                        .w(relative(1. / placement.columns as f32))
                        .px(px(1.5))
                        .overflow_hidden()
                        .child(
                            menu(
                                ("grid-event-menu", index),
                                trigger,
                                self.event_items(&e),
                                move |index, window, cx| {
                                    let _ = target.update(cx, |app, cx| {
                                        app.event_action(action.clone(), index, window, cx)
                                    });
                                },
                            )
                            .mouse_button(MouseButton::Right),
                        ),
                );
            }
            let day_start = date.and_hms_opt(0, 0, 0).unwrap();
            let day_end = day_start + Duration::days(1);
            if let Some((s, e)) = self.selection.span()
                && e > day_start
                && s < day_end
            {
                let start = (s.max(day_start) - day_start).num_minutes() as f32 / 60.;
                let end = (e.min(day_end) - day_start).num_minutes() as f32 / 60.;
                let highlighted = div()
                    .debug_selector(move || format!("time-selection-{di}"))
                    .absolute()
                    .left(px(2.))
                    .right(px(2.))
                    .top(rems(start * HOUR))
                    .h(rems((end - start) * HOUR))
                    .border_1()
                    .border_color(t.accent)
                    .bg(t.accent.opacity(0.12))
                    .child(
                        div()
                            .px(rems(0.3))
                            .text_size(rems(0.64))
                            .text_color(t.accent)
                            .child(self.selection.label()),
                    );
                col = col.child(highlighted);
            }
            if date == today {
                let now = Utc::now().with_timezone(&tz);
                let minute = now.hour() as f32 * 60. + now.minute() as f32;
                col = col.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(rems(minute / 60. * HOUR))
                        .h(px(1.))
                        .bg(t.accent)
                        .child(
                            div()
                                .absolute()
                                .left(-px(3.))
                                .top(-px(2.))
                                .size(px(5.))
                                .rounded_full()
                                .bg(t.accent),
                        ),
                );
            }
            let slot_target = cx.entity().downgrade();
            let open_target = slot_target.clone();
            let current = Slot::new(date, 9 * 60);
            col = col
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |app, event: &gpui_kit::MouseDownEvent, window, cx| {
                        if app.editor.is_some() {
                            return;
                        }
                        if let Some(slot) = app.pointer_slot(event.position) {
                            app.selected = None;
                            app.date = slot.day;
                            if event.modifiers.shift {
                                app.selection.begin(slot);
                                app.dragging = true;
                            } else {
                                app.selection.cursor = slot;
                                app.selection.anchor = None;
                                if event.click_count == 2 {
                                    app.new_event(false, window, cx);
                                }
                            }
                            window.focus(&app.focus, cx);
                            cx.notify();
                        }
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |app, event: &gpui_kit::MouseDownEvent, _, cx| {
                        if let Some(slot) = app.pointer_slot(event.position) {
                            app.selection.cursor = slot;
                            app.selection.anchor = None;
                            app.date = slot.day;
                            if let Some(bounds) = app.grid_bounds.get(&slot.day) {
                                app.slot_anchor = ((event.position.x - bounds.left())
                                    / bounds.size.width)
                                    .clamp(0., 1.);
                            }
                            app.slot_menu = true;
                            cx.stop_propagation();
                        }
                        cx.notify();
                    }),
                );
            let _ = current;
            let items = vec![
                MenuItem::new("New event")
                    .icon(IconName::Plus)
                    .shortcut("C"),
                MenuItem::new("New task").icon(IconName::SquareCheck),
                MenuItem::new("Go to today")
                    .separator_before()
                    .icon(IconName::CalendarCheck)
                    .shortcut("T"),
            ];
            col = col.child(
                div()
                    .absolute()
                    .left(relative(self.slot_anchor))
                    .top(rems(self.selection.cursor.minute as f32 / 60. * HOUR))
                    .size(px(1.))
                    .child(
                        menu(
                            format!(
                                "slot-menu-{di}-{}-{}",
                                self.selection.cursor.minute,
                                (self.slot_anchor * 1000.) as u32
                            ),
                            ContextTarget(div().size(px(1.))),
                            items,
                            move |index, window, cx| {
                                let _ = slot_target.update(cx, |app, cx| match index {
                                    0 => app.new_event(false, window, cx),
                                    1 => app.new_event(true, window, cx),
                                    _ => app.choose_date(
                                        Utc::now().with_timezone(&app.tz()).date_naive(),
                                        cx,
                                    ),
                                });
                            },
                        )
                        .mouse_button(MouseButton::Right)
                        .open(self.slot_menu && date == self.selection.cursor.day)
                        .on_open_change(move |open, _, cx| {
                            let _ = open_target.update(cx, |app, cx| {
                                app.slot_menu = *open;
                                cx.notify();
                            });
                        }),
                    ),
            );
            grid = grid.child(div().flex_1().min_w_0().child(col));
        }
        let first = self.initial_grid_scroll;
        self.initial_grid_scroll = false;
        let initial_scroll = self.grid_scroll.clone();
        let scroll = div()
            .on_prepaint(move |_, window, _| {
                if first {
                    initial_scroll
                        // Stop just above 07:00 so its centred label isn't clipped.
                        .set_offset(point(
                            px(0.),
                            -rems(HOUR * 7. - 0.6).to_pixels(window.rem_size()),
                        ));
                    window.refresh();
                }
            })
            .id("time-scroll")
            .debug_selector(|| "time-scroll".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.grid_scroll)
            .child(grid)
            .on_mouse_move(cx.listener(|app, event: &gpui_kit::MouseMoveEvent, _, cx| {
                // Pointer moves arrive far more often than the 15-minute slot changes.
                if app.dragging
                    && let Some(slot) = app.pointer_slot(event.position)
                    && slot != app.selection.cursor
                {
                    app.selection.cursor = slot;
                    app.date = slot.day;
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|app, _, window, cx| {
                    if app.dragging {
                        app.dragging = false;
                        app.new_event(false, window, cx);
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|app, _, window, cx| {
                    if app.dragging {
                        app.dragging = false;
                        app.new_event(false, window, cx);
                    }
                }),
            );
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(all_day)
            .child(scroll)
            .into_any_element()
    }
    fn month(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let days = month_days(self.date);
        let events = self.events(days[0], days[41] + Duration::days(1), cx);
        let compact = window.viewport_size().width < px(COMPACT * self.zoom);
        let mut view = div().flex_1().min_h_0().flex().flex_col().child(
            div()
                .flex()
                .border_b_1()
                .border_color(t.divider())
                .children(
                    ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                        .into_iter()
                        .map(|s| {
                            div()
                                .flex_1()
                                .py(rems(0.6))
                                .text_center()
                                .text_color(t.secondary)
                                .text_size(rems(0.7))
                                .child(s)
                        }),
                ),
        );
        let today = Utc::now().with_timezone(&self.tz()).date_naive();
        for (week, dates) in days.chunks(7).enumerate() {
            let mut row = div().flex().flex_1().min_h_0();
            for (i, date) in dates.iter().enumerate() {
                let date = *date;
                let mut cell = div()
                    .id(("month-day", week * 7 + i))
                    .debug_selector(move || format!("month-day-{}", week * 7 + i))
                    .w_full()
                    .h(self.month_cell_height)
                    .min_w_0()
                    .min_h_0()
                    .border_b_1()
                    .border_l_1()
                    .border_color(t.divider())
                    .bg(if date == self.date {
                        t.accent.opacity(0.06)
                    } else if date.month() != self.date.month() {
                        t.inset
                    } else {
                        t.background
                    })
                    .p(rems(if compact { 0.15 } else { 0.4 }))
                    .flex()
                    .flex_col()
                    .gap(rems(0.2))
                    .overflow_hidden()
                    .child(
                        button(
                            ("month-date", week * 7 + i),
                            "",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .accessibility_label(date.format("%A %-d %B").to_string())
                        .border_0()
                        .p(rems(0.))
                        .h(rems(1.5))
                        .bg(t.foreground.opacity(0.))
                        .justify_end()
                        .child(day_number(
                            date.day(),
                            date == today,
                            if date.month() == self.date.month() {
                                t.foreground
                            } else {
                                t.secondary
                            },
                            rems(0.72),
                            rems(1.5),
                            &t,
                        ))
                        .on_click(cx.listener(
                            move |app, _, window, cx| {
                                app.choose_date(date, cx);
                                if compact {
                                    app.change_view(View::Day, window, cx);
                                }
                            },
                        )),
                    );
                let day_events: Vec<_> = events
                    .iter()
                    .filter(|e| !e.task && e.on_date(date, self.tz()))
                    .collect();
                for (j, e) in day_events
                    .iter()
                    .take(if compact { 2 } else { 3 })
                    .enumerate()
                {
                    cell = cell.child(self.event_chip(e, week * 700 + i * 100 + j, true, cx));
                }
                if day_events.len() > if compact { 2 } else { 3 } {
                    cell = cell.child(div().text_color(t.secondary).text_size(rems(0.61)).child(
                        format!("+{} more", day_events.len() - if compact { 2 } else { 3 }),
                    ));
                }
                let target = cx.entity().downgrade();
                cell = cell.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |app, event: &gpui_kit::MouseDownEvent, window, cx| {
                        app.choose_date(date, cx);
                        if event.click_count == 2 {
                            app.new_event(false, window, cx);
                        }
                    }),
                );
                let size_target = cx.entity().downgrade();
                row = row.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .on_prepaint(move |bounds, window, cx| {
                            let _ = size_target.update(cx, |app, cx| {
                                if app.month_cell_height != bounds.size.height {
                                    app.month_cell_height = bounds.size.height;
                                    cx.notify();
                                    window.refresh();
                                }
                            });
                        })
                        .child(
                            menu(
                                ("month-context", week * 7 + i),
                                ContextTarget(cell),
                                vec![
                                    MenuItem::new("New event").icon(IconName::Plus),
                                    MenuItem::new("New task").icon(IconName::SquareCheck),
                                ],
                                move |index, window, cx| {
                                    let _ = target.update(cx, |app, cx| {
                                        app.choose_date(date, cx);
                                        app.new_event(index == 1, window, cx);
                                    });
                                },
                            )
                            .mouse_button(MouseButton::Right),
                        ),
                );
            }
            view = view.child(row);
        }
        view.into_any_element()
    }
    fn agenda(&self, tasks: bool, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let mut events = self.events(self.date, self.date + Duration::days(60), cx);
        if tasks {
            events = self
                .snapshot
                .events
                .iter()
                .filter(|e| e.task && self.visible(&e.calendar))
                .cloned()
                .collect();
            events.sort_by_key(|e| (e.completed, e.start));
        }
        let mut view = div()
            .id("agenda-scroll")
            .debug_selector(|| "agenda-scroll".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.agenda_scroll)
            .p(rems(1.4))
            .child(
                div()
                    .max_w(rems(52.))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .gap(rems(0.45))
                    .children(events.iter().enumerate().flat_map(|(i, e)| {
                        let date = e.dates_in(self.tz()).0;
                        let heading =
                            (i == 0 || events[i - 1].dates_in(self.tz()).0 != date).then(|| {
                                div()
                                    .pt(rems(0.85))
                                    .pb(rems(0.35))
                                    .text_size(rems(0.75))
                                    .text_color(t.secondary)
                                    .child(date.format("%A, %-d %B").to_string())
                                    .into_any_element()
                            });
                        let e = e.clone();
                        let event = if e.task {
                            let changed = e.clone();
                            div()
                                .flex()
                                .gap(rems(0.5))
                                .items_center()
                                .child(
                                    tool_button(
                                        ("task-check", i),
                                        if e.completed {
                                            IconName::SquareCheck
                                        } else {
                                            IconName::Square
                                        },
                                        "Complete task",
                                        "",
                                        cx,
                                    )
                                    .on_click(cx.listener(
                                        move |app, _, _, cx| {
                                            let mut e = changed.clone();
                                            e.completed = !e.completed;
                                            app.dispatch(Command::Save {
                                                event: e,
                                                notify: false,
                                            });
                                            cx.notify();
                                        },
                                    )),
                                )
                                .child(div().flex_1().min_w_0().child(self.event_chip(
                                    &e,
                                    100000 + i,
                                    false,
                                    cx,
                                )))
                                .into_any_element()
                        } else {
                            self.event_chip(&e, 100000 + i, false, cx)
                        };
                        heading
                            .into_iter()
                            .chain(std::iter::once(event))
                            .collect::<Vec<_>>()
                    })),
            );
        if events.is_empty() {
            view = view.child(
                div()
                    .py(rems(3.))
                    .text_center()
                    .text_color(t.secondary)
                    .child(if tasks {
                        "No tasks here. Add one when you need it."
                    } else {
                        "No events here. A little breathing room."
                    }),
            );
        }
        view.into_any_element()
    }
    fn editor_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let Some(f) = self.editor.as_mut() else {
            return div().into_any_element();
        };
        let focused = f
            .fields
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
        let field = |i: usize, window: &mut Window, cx: &mut Context<Self>| {
            reveal_field(
                input(("editor-input", i), &f.fields[i], window, cx)
                    .debug_selector(move || format!("editor-input-{i}"))
                    .w_full()
                    .border_0()
                    .border_b_1()
                    .border_color(t.divider())
                    .bg(t.background),
                reveal == Some(i),
                &self.form_scroll,
            )
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
        let target = cx.entity().downgrade();
        let meeting_items = vec![
            MenuItem::new("No meeting link").checked(f.meeting == 0),
            MenuItem::new("Paste a meeting link")
                .icon(IconName::Link)
                .checked(f.meeting == 1),
            MenuItem::new("Generate Google Meet")
                .icon(IconName::Video)
                .checked(f.meeting == 2)
                .disabled(!can_meet),
        ];
        let meeting = menu(
            "meeting-options",
            button("meeting-trigger", "", ButtonVariant::Secondary, cx)
                .debug_selector(|| "meeting-trigger".into())
                .disabled(self.busy)
                .w_full()
                .border_0()
                .justify_start()
                .child(icon(IconName::Video).size(rems(0.9)))
                .child(
                    [
                        "Add a meeting link",
                        "Use a meeting link",
                        "Generate Google Meet",
                    ][f.meeting],
                )
                .child(icon(IconName::ChevronDown).size(rems(0.7))),
            meeting_items,
            move |index, window, cx| {
                let _ = target.update(cx, |app, cx| {
                    if !app.busy {
                        if let Some(f) = &mut app.editor {
                            f.meeting = index;
                            if index == 1 {
                                f.fields[6].update(cx, |s, cx| s.focus(window, cx));
                            }
                        }
                        cx.notify();
                    }
                });
            },
        );
        let calendar = if self.busy {
            div()
                .child(
                    f.calendar
                        .read(cx)
                        .selected()
                        .map(|s| s.label.clone())
                        .unwrap_or_default(),
                )
                .into_any_element()
        } else {
            select("editor-calendar", &f.calendar, window, cx).into_any_element()
        };
        let repeats = if self.busy {
            div()
                .child(
                    f.repeat
                        .read(cx)
                        .selected()
                        .map(|s| s.label.clone())
                        .unwrap_or_default(),
                )
                .into_any_element()
        } else {
            select("editor-repeat", &f.repeat, window, cx).into_any_element()
        };
        let mut body = div()
            .id("editor-scroll")
            .debug_selector(|| "editor-scroll".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.form_scroll)
            .px(rems(1.))
            .py(rems(0.5))
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .child(field(0, window, cx))
            .child(row("Calendar", calendar, cx))
            .child(
                button("all-day", "All day", ButtonVariant::Secondary, cx)
                    .child(
                        icon(if f.all_day {
                            IconName::SquareCheck
                        } else {
                            IconName::Square
                        })
                        .size(rems(0.9)),
                    )
                    .disabled(self.busy)
                    .border_0()
                    .justify_start()
                    .on_click(cx.listener(|app, _, _, cx| {
                        if let Some(f) = &mut app.editor {
                            f.all_day = !f.all_day;
                        }
                        cx.notify();
                    })),
            )
            .child(row(
                if f.base.task { "Due" } else { "Starts" },
                div()
                    .flex()
                    .gap(rems(0.4))
                    .child(div().flex_1().min_w_0().child(field(1, window, cx)))
                    .children((!f.all_day).then(|| div().w(rems(5.2)).child(field(2, window, cx))))
                    .into_any_element(),
                cx,
            ));
        if !f.base.task {
            body = body.child(row(
                "Ends",
                div()
                    .flex()
                    .gap(rems(0.4))
                    .child(div().flex_1().min_w_0().child(field(3, window, cx)))
                    .children((!f.all_day).then(|| div().w(rems(5.2)).child(field(4, window, cx))))
                    .into_any_element(),
                cx,
            ));
        }
        body = body.child(field(5, window, cx)).child(meeting);
        if f.meeting == 1 {
            body = body.child(field(6, window, cx));
        } else if f.meeting == 2 {
            body = body.child(
                div()
                    .text_size(rems(0.68))
                    .text_color(t.secondary)
                    .whitespace_normal()
                    .child("Google will create a Meet link when you save."),
            );
        }
        if f.base.series_id.is_some() {
            body = body.child(
                div()
                    .text_size(rems(0.68))
                    .text_color(t.secondary)
                    .whitespace_normal()
                    .child(if f.base.remote_id.is_some() {
                        "Changes apply to this occurrence."
                    } else {
                        "Changes apply to the entire series."
                    }),
            );
        }
        if google && !f.base.task {
            body = body.child(field(7, window, cx));
            if !f.fields[7].read(cx).value().is_empty() {
                body = body.child(
                    button(
                        "notify-guests",
                        if f.notify {
                            "Notify guests ✓"
                        } else {
                            "Do not notify guests"
                        },
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .disabled(self.busy)
                    .border_0()
                    .on_click(cx.listener(|app, _, _, cx| {
                        if let Some(f) = &mut app.editor {
                            f.notify = !f.notify;
                        }
                        cx.notify();
                    })),
                );
            }
        }
        body = body.child(
            button(
                "more-details",
                if f.details {
                    "Fewer details"
                } else {
                    "More details"
                },
                ButtonVariant::Secondary,
                cx,
            )
            .disabled(self.busy)
            .border_0()
            .justify_start()
            .child(
                icon(if f.details {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .size(rems(0.7)),
            )
            .on_click(cx.listener(|app, _, _, cx| {
                if let Some(f) = &mut app.editor {
                    f.details = !f.details;
                }
                cx.notify();
            })),
        );
        if f.details {
            if f.base.series_id.is_none() {
                body = body.child(row("Repeat", repeats, cx));
            }
            body = body
                .child(row("Time zone", field(8, window, cx), cx))
                .child(reveal_field(
                    textarea("editor-notes", &f.notes, window, cx)
                        .disabled(self.busy)
                        .w_full()
                        .border_0()
                        .bg(t.background),
                    reveal == Some(9),
                    &self.form_scroll,
                ));
        }
        if self.error {
            body = body.child(
                div()
                    .text_color(t.danger)
                    .text_size(rems(0.72))
                    .whitespace_normal()
                    .child(self.status.clone()),
            );
        }
        let title = if f.editing {
            if f.base.task {
                "Edit task"
            } else {
                "Edit event"
            }
        } else if f.base.task {
            "New task"
        } else {
            "New event"
        };
        let save_label = if self.busy {
            "Saving…"
        } else if google && f.notify && !f.fields[7].read(cx).value().trim().is_empty() {
            "Save & invite"
        } else {
            "Save"
        };
        let discard = f.discard;
        let footer = if discard {
            div()
                .p(rems(1.))
                .border_t_1()
                .border_color(t.divider())
                .flex()
                .flex_col()
                .gap(rems(0.6))
                .child(
                    div()
                        .text_size(rems(0.75))
                        .child("Discard your unsaved changes?"),
                )
                .child(
                    div()
                        .flex()
                        .gap(rems(0.5))
                        .child(
                            button("keep-editing", "Keep editing", ButtonVariant::Primary, cx)
                                .on_click(cx.listener(|app, _, _, cx| {
                                    if let Some(f) = &mut app.editor {
                                        f.discard = false;
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            button("discard-editor", "Discard", ButtonVariant::Secondary, cx)
                                .text_color(t.danger)
                                .on_click(cx.listener(|app, _, window, cx| {
                                    app.editor = None;
                                    app.selection.anchor = None;
                                    window.focus(&app.focus, cx);
                                    cx.notify();
                                })),
                        ),
                )
        } else {
            div()
                .p(rems(1.))
                .border_t_1()
                .border_color(t.divider())
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_size(rems(0.65))
                        .text_color(t.secondary)
                        .child("Ctrl Enter to save"),
                )
                .child(
                    button("save-event", save_label, ButtonVariant::Primary, cx)
                        .debug_selector(|| "save-event".into())
                        .disabled(self.busy)
                        .on_click(cx.listener(|app, _, _, cx| app.save(cx))),
                )
        };
        let f = self.editor.as_ref().unwrap();
        focus_scope("event-editor")
            .debug_selector(|| "event-editor".into())
            .role(gpui_kit::Role::Dialog)
            .aria_label(title)
            .key_context("EventEditor")
            .track_focus(&f.focus)
            .absolute()
            .right(px(12.))
            .top(rems(4.5))
            .w(rems(28.))
            .max_w(window.viewport_size().width - px(24.))
            .h(
                (window.viewport_size().height - rems(6.).to_pixels(window.rem_size())).min(
                    rems(if f.details {
                        42.
                    } else if f.meeting > 0 {
                        33.
                    } else {
                        29.
                    })
                    .to_pixels(window.rem_size()),
                ),
            )
            .min_h(px(120.))
            .flex()
            .flex_col()
            .border_1()
            .border_color(t.control_border())
            .bg(t.background)
            .shadow_lg()
            .occlude()
            .child(
                div()
                    .px(rems(1.))
                    .py(rems(0.75))
                    .flex_shrink_0()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(rems(0.9))
                            .child(title),
                    )
                    .child(
                        tool_button("close-editor", IconName::X, "Close editor", "Esc", cx)
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|app, _, window, cx| app.close_editor(window, cx)),
                            ),
                    ),
            )
            .child(body)
            .child(footer.flex_shrink_0())
            .into_any_element()
    }
    fn details(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(e) = self.selected.as_ref() else {
            return div().into_any_element();
        };
        let t = cx.omarchy().clone();
        let writable = self.writable(e);
        let color = self.calendar_color(&e.calendar, cx);
        let target = cx.entity().downgrade();
        let action = e.clone();
        let open_target = target.clone();
        let more = menu(
            "details-menu",
            tool_button(
                "details-more",
                IconName::Ellipsis,
                "Event actions",
                "Shift F10",
                cx,
            ),
            self.event_items(e),
            move |index, window, cx| {
                let _ = target.update(cx, |app, cx| {
                    app.event_action(action.clone(), index, window, cx)
                });
            },
        )
        .open(self.event_menu)
        .on_open_change(move |open, _, cx| {
            let _ = open_target.update(cx, |app, cx| {
                app.event_menu = *open;
                cx.notify();
            });
        });
        let mut content = div()
            .id("details-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(rems(1.1))
            .flex()
            .flex_col()
            .gap(rems(1.))
            .child(
                div()
                    .text_size(rems(1.25))
                    .font_weight(FontWeight::SEMIBOLD)
                    .whitespace_normal()
                    .child(e.title.clone()),
            )
            .child(detail_line(
                IconName::Clock,
                format!(
                    "{}\n{}",
                    e.dates_in(self.tz()).0.format("%A, %-d %B %Y"),
                    e.time_label(self.tz())
                ),
                cx,
            ))
            .child(detail_line(
                IconName::Calendar,
                self.snapshot
                    .calendars
                    .iter()
                    .find(|c| c.id == e.calendar)
                    .map(|c| c.display_name())
                    .unwrap_or_default(),
                cx,
            ))
            .child(detail_line(IconName::Globe, e.timezone.clone(), cx));
        if !e.location.is_empty() {
            content = content.child(detail_line(IconName::MapPin, e.location.clone(), cx));
        }
        if !e.link.is_empty() {
            let link = e.link.clone();
            content = content.child(
                action_button(
                    "join-meeting",
                    IconName::Video,
                    "Join meeting",
                    ButtonVariant::Primary,
                    cx,
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    match ocal_core::safe_url(&link) {
                        Ok(url) => {
                            if let Err(error) = std::process::Command::new("xdg-open")
                                .arg(url.as_str())
                                .spawn()
                            {
                                app.status = error.to_string();
                                app.error = true;
                            }
                        }
                        Err(error) => {
                            app.status = error.to_string();
                            app.error = true;
                        }
                    }
                    cx.notify();
                })),
            );
        } else if e.conference_status == "pending" {
            content = content.child(detail_line(
                IconName::Video,
                "Google Meet link is being generated. It will appear here automatically.".into(),
                cx,
            ));
        } else if e.conference_status == "failure" {
            content = content.child(
                div()
                    .text_color(t.danger)
                    .child("Google could not generate this Meet link."),
            );
        }
        if !e.notes.is_empty() {
            content = content.child(
                div()
                    .text_size(rems(0.78))
                    .whitespace_normal()
                    .child(e.notes.clone()),
            );
        }
        if e.series_id.is_some() || e.repeat != Repeat::Never {
            content = content.child(detail_line(
                IconName::Repeat,
                if e.remote_id.is_some() {
                    "Part of a recurring event"
                } else {
                    "Recurring series"
                }
                .into(),
                cx,
            ));
        }
        for attendee in &e.attendees {
            content = content.child(
                div()
                    .text_size(rems(0.72))
                    .whitespace_normal()
                    .child(format!("{} · {}", attendee.email, attendee.response)),
            );
        }
        if e.remote_id.is_some() && e.attendees.iter().any(|a| a.me) {
            let ev = e.clone();
            content = content.child(
                div().flex().gap(rems(0.3)).children(
                    [
                        ("Yes", "accepted"),
                        ("Maybe", "tentative"),
                        ("No", "declined"),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(i, (label, response))| {
                        let e = ev.clone();
                        button(("rsvp", i), label, ButtonVariant::Secondary, cx)
                            .disabled(self.busy)
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.busy = true;
                                app.dispatch(Command::Respond(e.clone(), response.into()));
                                cx.notify();
                            }))
                    }),
                ),
            );
        }
        let extra = f32::from(!e.location.is_empty()) * 2.5
            + f32::from(!e.link.is_empty() || !e.conference_status.is_empty()) * 3.
            + f32::from(e.repeat != Repeat::Never || e.series_id.is_some()) * 2.
            + e.attendees.len() as f32 * 1.8
            + f32::from(e.attendees.iter().any(|a| a.me)) * 3.
            + if e.notes.is_empty() {
                0.
            } else {
                2. + e.notes.chars().count() as f32 / 40. * 1.3
            };
        let edit = e.clone();
        let delete = e.clone();
        let footer = if self.delete_confirm {
            div()
                .p(rems(1.))
                .flex()
                .flex_col()
                .gap(rems(0.5))
                .child(div().text_size(rems(0.74)).whitespace_normal().child(
                    if e.series_id.is_some() && e.remote_id.is_none() {
                        "Delete every event in this series?"
                    } else {
                        "Delete this event?"
                    },
                ))
                .child(
                    div()
                        .flex()
                        .gap(rems(0.5))
                        .child(
                            button("confirm-delete", "Delete", ButtonVariant::Primary, cx)
                                .disabled(self.busy)
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.busy = true;
                                    app.dispatch(Command::Delete(delete.clone()));
                                    cx.notify();
                                })),
                        )
                        .child(
                            button("cancel-delete", "Cancel", ButtonVariant::Secondary, cx)
                                .on_click(cx.listener(|app, _, _, cx| {
                                    app.delete_confirm = false;
                                    cx.notify();
                                })),
                        ),
                )
        } else {
            div().p(rems(1.)).child(
                action_button(
                    "edit-event",
                    IconName::PencilLine,
                    "Edit",
                    ButtonVariant::Primary,
                    cx,
                )
                .disabled(!writable || self.busy)
                .on_click(cx.listener(move |app, _, window, cx| {
                    app.open_editor(edit.clone(), true, window, cx)
                })),
            )
        };
        div()
            .id("event-details")
            .debug_selector(|| "event-details".into())
            .role(gpui_kit::Role::Dialog)
            .aria_label("Event details")
            .absolute()
            .right(px(12.))
            .top(rems(4.5))
            .w(rems(25.))
            .max_w(window.viewport_size().width - px(24.))
            .h(
                (window.viewport_size().height - rems(6.).to_pixels(window.rem_size()))
                    .min(rems((18. + extra).min(34.)).to_pixels(window.rem_size())),
            )
            .flex()
            .flex_col()
            .border_1()
            .border_t_2()
            .border_color(color)
            .bg(t.background)
            .shadow_lg()
            .occlude()
            .child(
                div()
                    .px(rems(0.7))
                    .py(rems(0.45))
                    .flex()
                    .justify_end()
                    .gap(rems(0.3))
                    .child(more)
                    .child(
                        tool_button("close-details", IconName::X, "Close details", "Esc", cx)
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.selected = None;
                                app.event_menu = false;
                                window.focus(&app.focus, cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(content)
            .child(footer.border_t_1().border_color(t.divider()))
            .into_any_element()
    }
    fn settings_view(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let google_ready = ocal_core::oauth::ClientConfig::bundled().is_ok();
        let mut view = div()
            .id("settings-scroll")
            .flex_1().min_h_0().overflow_y_scroll()
            .p(rems(2.)).flex().flex_col().gap(rems(1.))
            .child(div().text_size(rems(1.4)).child("A calendar that feels at home."))
            .child(div().text_color(t.secondary).whitespace_normal().child(
                "Google Calendar accounts and local calendars live side by side. Your account credentials stay in the desktop keyring."
            ));
        for (i, account) in self.snapshot.accounts.iter().enumerate() {
            let count = self
                .snapshot
                .calendars
                .iter()
                .filter(|calendar| calendar.account.as_deref() == Some(account.id.as_str()))
                .count();
            view = view.child(
                div()
                    .debug_selector(move || format!("google-account-{i}"))
                    .w_full()
                    .p(rems(0.8))
                    .border_1()
                    .border_color(t.divider())
                    .rounded(rems(0.4))
                    .bg(t.inset)
                    .flex()
                    .flex_col()
                    .gap(rems(0.25))
                    .child(div().whitespace_normal().child(account.id.clone()))
                    .child(
                        div()
                            .text_color(t.secondary)
                            .text_size(rems(0.7))
                            .child(format!(
                                "Google · {count} {}",
                                if count == 1 { "calendar" } else { "calendars" }
                            )),
                    ),
            );
        }
        view = view
            .child(action_button("connect-google", IconName::Plus,
                if self.busy { "Connecting…" } else if self.snapshot.accounts.is_empty() {
                    "Copy Google sign-in link"
                } else { "Add Google account" },
                ButtonVariant::Primary, cx)
                .debug_selector(|| "connect-google".into())
                .disabled(self.demo || self.busy || !google_ready)
                .on_click(cx.listener(|app, _, _, cx| {
                    app.busy = true;
                    app.error = false;
                    app.sign_in_url = None;
                    app.cancel_sign_in.store(false, Ordering::Relaxed);
                    app.status = "Preparing Google sign-in link…".into();
                    app.dispatch(Command::Connect);
                    cx.notify();
                })))
            .child(div().text_color(t.secondary).text_size(rems(0.75)).whitespace_normal().child(
                if self.demo {
                    "This demo is isolated. Run ocal without --demo to connect an account."
                } else if !google_ready {
                    "Google sign-in isn’t available in this development build yet. Local calendars work immediately."
                } else {
                    "Copy the sign-in link, paste it into your preferred browser profile, and choose an account. Keep Ocal open while signing in."
                }
            ))
            .children(self.sign_in_url.as_ref().map(|url| {
                let copy_url = url.clone();
                let open_url = url.clone();
                div().flex().flex_col().gap(rems(0.5))
                    .child(div().flex().flex_wrap().gap(rems(0.5))
                        .child(action_button("copy-sign-in-link", IconName::Copy, "Copy again", ButtonVariant::Secondary, cx)
                            .debug_selector(|| "copy-sign-in-link".into())
                            .on_click(cx.listener(move |app, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy_url.clone()));
                                app.status = "Sign-in link copied · Paste it into your preferred browser profile".into();
                                cx.notify();
                            })))
                        .child(button("open-sign-in-link", "Open default browser", ButtonVariant::Secondary, cx)
                            .on_click(cx.listener(move |app, _, _, cx| {
                                if std::process::Command::new("xdg-open").arg(&open_url).spawn().is_err() {
                                    app.status = "Could not open the browser. Use Copy again and paste the link into your browser".into();
                                    cx.notify();
                                }
                            })))
                        .child(button("cancel-sign-in", "Cancel", ButtonVariant::Secondary, cx)
                            .debug_selector(|| "cancel-sign-in".into())
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.cancel_sign_in.store(true, Ordering::Relaxed);
                                app.sign_in_url = None;
                                app.status = "Cancelling sign-in…".into();
                                cx.notify();
                            }))))
                    .child(div().text_color(t.secondary).text_size(rems(0.75)).whitespace_normal()
                        .child("This link stays active for 10 minutes. Copy again uses the same sign-in attempt."))
            }))
            .children((self.busy || self.error).then(|| {
                div().id("account-status")
                    .debug_selector(|| "account-status".into())
                    .w_full().p(rems(0.8)).border_1().rounded(rems(0.4))
                    .border_color(if self.error { t.danger } else { t.divider() })
                    .bg(t.inset).text_color(if self.error { t.danger } else { t.secondary })
                    .whitespace_normal().child(self.status.clone())
            }))
            .child(action_button("advanced-login", if self.advanced_login {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            }, "Advanced connection options", ButtonVariant::Secondary, cx)
                .debug_selector(|| "advanced-login".into())
                .on_click(cx.listener(|app, _, _, cx| {
                    app.advanced_login = !app.advanced_login;
                    cx.notify();
                })));
        if self.advanced_login {
            view = view
                .child(div().text_color(t.secondary).text_size(rems(0.75)).whitespace_normal()
                    .child("For developers and custom builds: use your own Google Desktop app client file."))
                .child(button("connect-google-custom", "Use a custom client file…", ButtonVariant::Secondary, cx)
                    .debug_selector(|| "connect-google-custom".into())
                    .disabled(self.demo || self.busy)
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.busy = true;
                        app.error = false;
                        app.sign_in_url = None;
                        app.cancel_sign_in.store(false, Ordering::Relaxed);
                        app.status = "Choose a Google client file…".into();
                        app.dispatch(Command::ConnectCustom);
                        cx.notify();
                    })));
        }
        view.child(
            div()
                .mt(rems(1.))
                .text_size(rems(0.95))
                .child("Calendar sets"),
        )
        .child(
            div()
                .text_color(t.secondary)
                .text_size(rems(0.75))
                .whitespace_normal()
                .child("Save the calendars currently checked in the sidebar as a set."),
        )
        .child(input("set-name", &self.set_name, window, cx))
        .child(
            button(
                "save-set",
                "Save current calendars",
                ButtonVariant::Secondary,
                cx,
            )
            .on_click(cx.listener(|app, _, _, cx| {
                let name = app.set_name.read(cx).value().trim().to_owned();
                if name.is_empty() {
                    app.status = "Name your calendar set".into();
                    app.error = true;
                } else {
                    app.dispatch(Command::SaveSet(CalendarSet {
                        id: uuid::Uuid::new_v4().to_string(),
                        name,
                        calendars: app
                            .snapshot
                            .calendars
                            .iter()
                            .filter(|c| c.visible)
                            .map(|c| c.id.clone())
                            .collect(),
                    }));
                }
                cx.notify();
            })),
        )
        .child(
            div()
                .mt(rems(1.))
                .text_size(rems(0.95))
                .child("Display time zone"),
        )
        .child(
            div()
                .text_color(t.secondary)
                .child(self.display_zone.clone()),
        )
        .child(div().flex().flex_wrap().gap(rems(0.4)).children({
            let system = system_zone();
            let mut zones = vec![system.clone()];
            for zone in ["Europe/London", "America/New_York", "UTC"] {
                if zone != system {
                    zones.push(zone.into());
                }
            }
            zones
                .into_iter()
                .enumerate()
                .map(|(i, name)| {
                    let label = if name == system {
                        format!("{name} (system)")
                    } else {
                        name.clone()
                    };
                    button(("timezone", i), label, ButtonVariant::Secondary, cx)
                        .when(name == self.display_zone, |b| b.bg(t.selection))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.display_zone = name.clone();
                            app.reveal_cursor = true;
                            cx.notify();
                        }))
                })
                .collect::<Vec<_>>()
        }))
        .child(
            button(
                "back-to-calendar",
                "Back to calendar",
                ButtonVariant::Secondary,
                cx,
            )
            .on_click(cx.listener(|app, _, window, cx| {
                app.settings = false;
                window.focus(&app.focus, cx);
                cx.notify();
            })),
        )
        .into_any_element()
    }
    fn help_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        div()
            .id("help-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(rems(2.))
            .flex()
            .flex_col()
            .gap(rems(0.8))
            .child(
                div()
                    .text_size(rems(1.4))
                    .child("Move through your calendar."),
            )
            .children(
                [
                    (
                        "↑ / ↓",
                        "Move the time cursor by 15 minutes; highlight events",
                    ),
                    ("← / →", "Previous / next day"),
                    ("Shift + arrows", "Extend the time selection"),
                    (
                        "Enter",
                        "Open the highlighted event or add at the cursor / range",
                    ),
                    (
                        "Shift + drag",
                        "Select a range with the mouse, then fill in details",
                    ),
                    ("Double-click / right-click", "Add an event at that time"),
                    ("c / Ctrl N", "New event"),
                    ("1 / 2 / 3 / 4 / 5", "Day / week / month / agenda / tasks"),
                    ("[ / ]", "Previous / next period"),
                    ("t", "Go to today"),
                    ("/ / Ctrl F", "Search cached events"),
                    ("b", "Calendars in a narrow window"),
                    ("Menu / Shift F10", "Context menu"),
                    ("Ctrl Enter", "Save event"),
                    ("Esc", "Dismiss / cancel selection"),
                    ("Ctrl + / − / 0", "Interface zoom"),
                    ("Ctrl R", "Sync Google calendars"),
                ]
                .into_iter()
                .map(|(key, label)| {
                    div()
                        .flex()
                        .gap(rems(1.))
                        .child(div().w(rems(10.)).text_color(t.accent).child(key))
                        .child(div().flex_1().min_w_0().whitespace_normal().child(label))
                }),
            )
            .child(
                button(
                    "close-help",
                    "Back to calendar",
                    ButtonVariant::Secondary,
                    cx,
                )
                .on_click(cx.listener(|app, _, window, cx| {
                    app.help = false;
                    window.focus(&app.focus, cx);
                    cx.notify();
                })),
            )
            .into_any_element()
    }
}
/// Include calendar-owned account IDs as well as saved accounts, so cached calendars
/// remain visible even if an older build did not persist their account record.
fn calendar_groups(snapshot: &Snapshot) -> Vec<(Option<String>, Vec<usize>)> {
    let mut accounts: std::collections::BTreeSet<String> = snapshot
        .accounts
        .iter()
        .map(|account| account.id.clone())
        .collect();
    accounts.extend(
        snapshot
            .calendars
            .iter()
            .filter_map(|calendar| calendar.account.clone()),
    );
    std::iter::once(None)
        .chain(accounts.into_iter().map(Some))
        .map(|account| {
            let indices = snapshot
                .calendars
                .iter()
                .enumerate()
                .filter(|(_, calendar)| calendar.account == account)
                .map(|(i, _)| i)
                .collect();
            (account, indices)
        })
        .collect()
}

/// An opaque calendar-coloured fill over the theme background.
fn tint(t: &gpui_omarchy::Theme, color: gpui_kit::Hsla, amount: f32) -> gpui_kit::Hsla {
    t.background.blend(color.opacity(amount))
}
/// Calendar colours are often pastel; on light themes they fail contrast as text.
fn event_text(t: &gpui_omarchy::Theme, color: gpui_kit::Hsla) -> gpui_kit::Hsla {
    if t.background.l > 0.5 {
        t.bright
    } else {
        color
    }
}
/// A day number, filled with the accent colour when it is today.
fn day_number(
    day: u32,
    today: bool,
    color: gpui_kit::Hsla,
    size: gpui_kit::Rems,
    badge: gpui_kit::Rems,
    t: &gpui_omarchy::Theme,
) -> gpui_kit::Div {
    div()
        .min_w(badge)
        .h(badge)
        .px(rems(0.3))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .text_size(size)
        .when(today, |d| {
            d.bg(t.accent)
                .text_color(t.on_accent)
                .font_weight(FontWeight::SEMIBOLD)
        })
        .when(!today, |d| d.text_color(color))
        .child(day.to_string())
}
fn row(label: &'static str, content: AnyElement, cx: &App) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(rems(0.6))
        .child(
            div()
                .w(rems(4.2))
                .flex_shrink_0()
                .text_size(rems(0.72))
                .text_color(cx.omarchy().secondary)
                .child(label),
        )
        .child(div().flex_1().min_w_0().child(content))
        .into_any_element()
}
fn detail_line(glyph: IconName, text: String, cx: &App) -> AnyElement {
    div()
        .flex()
        .gap(rems(0.7))
        .items_start()
        .child(
            icon(glyph)
                .size(rems(0.95))
                .text_color(cx.omarchy().secondary),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(rems(0.78))
                .whitespace_normal()
                .child(text),
        )
        .into_any_element()
}
impl Focusable for Ocal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for Ocal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.restore_focus {
            window.focus(&self.focus, cx);
            self.restore_focus = false;
        }
        window.set_rem_size(px(16. * self.zoom));
        let t = cx.omarchy().clone();
        let narrow = window.viewport_size().width < px(NARROW * self.zoom);
        let compact = window.viewport_size().width < px(COMPACT * self.zoom);
        let target = cx.entity().downgrade();
        let more = menu(
            "app-menu",
            tool_button(
                "app-actions",
                IconName::Ellipsis,
                "Calendar options",
                "",
                cx,
            ),
            vec![
                MenuItem::new("Settings").icon(IconName::Settings),
                MenuItem::new("Import iCalendar…").icon(IconName::Upload),
                MenuItem::new("Export visible calendars…").icon(IconName::Download),
                MenuItem::new("Keyboard shortcuts")
                    .separator_before()
                    .icon(IconName::CircleQuestionMark)
                    .shortcut("?"),
            ],
            move |index, window, cx| {
                let _ = target.update(cx, |app, cx| {
                    if app.editor.is_some() {
                        return;
                    }
                    match index {
                        0 => {
                            app.settings = true;
                            app.help = false;
                            app.selected = None;
                            window.focus(&app.focus, cx);
                        }
                        1 => app.dispatch(Command::Import),
                        2 => app.dispatch(Command::Export(
                            app.snapshot
                                .events
                                .iter()
                                .filter(|e| app.visible(&e.calendar))
                                .cloned()
                                .collect(),
                        )),
                        _ => {
                            app.help = true;
                            app.settings = false;
                            app.selected = None;
                        }
                    }
                    cx.notify();
                });
            },
        );
        let toolbar = div()
            .h(rems(3.2))
            .flex_shrink_0()
            .px(rems(0.85))
            .border_b_1()
            .border_color(t.divider())
            .bg(t.inset)
            .flex()
            .items_center()
            .gap(rems(0.5))
            .children((!narrow).then(|| {
                div()
                    .w(rems(14.8))
                    .flex_shrink_0()
                    .flex()
                    .gap(rems(0.6))
                    .items_center()
                    .child(
                        icon(IconName::CalendarRange)
                            .size(rems(1.05))
                            .text_color(t.accent),
                    )
                    .child("Ocal")
            }))
            .children(narrow.then(|| {
                tool_button("toggle-sidebar", IconName::PanelLeft, "Calendars", "B", cx).on_click(
                    cx.listener(|app, _, window, cx| {
                        if app.editor.is_none() {
                            app.sidebar_open = !app.sidebar_open;
                            window.focus(&app.focus, cx);
                            cx.notify();
                        }
                    }),
                )
            }))
            .child(
                tool_button("new-event", IconName::Plus, "New event", "C", cx)
                    .on_click(cx.listener(|app, _, window, cx| app.new_event(false, window, cx))),
            )
            .child(
                tool_button(
                    "previous",
                    IconName::ChevronLeft,
                    "Previous period",
                    "[",
                    cx,
                )
                .on_click(cx.listener(|app, _, _, cx| app.navigate(-1, cx))),
            )
            .child(
                button("today", "Today", ButtonVariant::Secondary, cx)
                    .border_0()
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.choose_date(Utc::now().with_timezone(&app.tz()).date_naive(), cx)
                    })),
            )
            .child(
                tool_button("next", IconName::ChevronRight, "Next period", "]", cx)
                    .on_click(cx.listener(|app, _, _, cx| app.navigate(1, cx))),
            )
            .child(div().flex_1().min_w_0())
            .children((!compact).then(|| {
                input("search", &self.search, window, cx)
                    .w(rems(18.))
                    .min_w(rems(9.))
                    .flex_shrink(1.)
                    .border_0()
                    .bg(t.surface)
            }))
            .child(
                tool_button("sync", IconName::RefreshCw, "Sync calendars", "Ctrl R", cx)
                    .disabled(self.busy)
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.dispatch(Command::Sync(app.date));
                        app.status = "Syncing…".into();
                        cx.notify();
                    })),
            )
            .child(more);
        let target = cx.entity().downgrade();
        let view_menu = menu(
            "view-menu",
            button(
                "view-trigger",
                self.view.label(),
                ButtonVariant::Secondary,
                cx,
            )
            .border_0()
            .child(icon(IconName::ChevronDown).size(rems(0.75))),
            [
                View::Day,
                View::Week,
                View::Month,
                View::Agenda,
                View::Tasks,
            ]
            .iter()
            .enumerate()
            .map(|(i, v)| {
                MenuItem::new(v.label())
                    .checked(*v == self.view)
                    .shortcut((i + 1).to_string())
            })
            .collect(),
            move |index, window, cx| {
                let _ = target.update(cx, |app, cx| {
                    app.change_view(
                        [
                            View::Day,
                            View::Week,
                            View::Month,
                            View::Agenda,
                            View::Tasks,
                        ][index],
                        window,
                        cx,
                    )
                });
            },
        );
        let heading = div()
            .h(rems(4.))
            .flex_shrink_0()
            .px(rems(1.35))
            .flex()
            .items_center()
            .justify_between()
            .gap(rems(0.5))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(rems(0.5))
                    .child(
                        div()
                            .text_size(rems(1.4))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.bright)
                            .child(self.date.format("%B").to_string()),
                    )
                    .child(
                        div()
                            .text_size(rems(1.4))
                            .text_color(t.accent)
                            .child(self.date.year().to_string()),
                    ),
            )
            .child(if compact {
                view_menu.into_any_element()
            } else {
                div()
                    .flex()
                    .gap(rems(0.25))
                    .children(
                        [
                            View::Day,
                            View::Week,
                            View::Month,
                            View::Agenda,
                            View::Tasks,
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| {
                            button(("view", i), v.label(), ButtonVariant::Secondary, cx)
                                .border_0()
                                .px(rems(0.75))
                                .bg(if v == self.view {
                                    t.selection
                                } else {
                                    t.foreground.opacity(0.)
                                })
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    app.change_view(v, window, cx)
                                }))
                        }),
                    )
                    .into_any_element()
            });
        let cursor_event = self.event_at_cursor(cx);
        let content = if self.settings {
            self.settings_view(window, cx)
        } else if self.help {
            self.help_view(cx)
        } else if !self.search.read(cx).value().trim().is_empty() {
            self.agenda(false, cx)
        } else {
            match self.view {
                View::Day | View::Week => self.grid(cursor_event.as_ref(), window, cx),
                View::Month => self.month(window, cx),
                View::Agenda => self.agenda(false, cx),
                View::Tasks => self.agenda(true, cx),
            }
        };
        let body = div()
            .flex_1()
            .min_h_0()
            .flex()
            .children((!narrow).then(|| self.sidebar(window, cx)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(heading)
                    .children(compact.then(|| {
                        div().px(rems(1.35)).pb(rems(0.6)).child(
                            input("compact-search", &self.search, window, cx)
                                .w_full()
                                .border_0()
                                .bg(t.surface),
                        )
                    }))
                    .child(content),
            );
        let mut root = focus_scope("ocal")
            .track_focus(&self.focus)
            .key_context("Ocal OmarchyFocusScope")
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(t.background)
            .text_color(t.foreground)
            .font_family(t.font.clone())
            .text_size(rems(0.8))
            .on_action(
                cx.listener(|app, _: &NewEvent, window, cx| app.new_event(false, window, cx)),
            )
            .on_action(cx.listener(|app, _: &Today, _, cx| {
                app.choose_date(Utc::now().with_timezone(&app.tz()).date_naive(), cx)
            }))
            .on_action(cx.listener(|app, _: &Previous, _, cx| app.navigate(-1, cx)))
            .on_action(cx.listener(|app, _: &Next, _, cx| app.navigate(1, cx)))
            .on_action(
                cx.listener(|app, _: &DayView, window, cx| app.change_view(View::Day, window, cx)),
            )
            .on_action(
                cx.listener(|app, _: &WeekView, window, cx| {
                    app.change_view(View::Week, window, cx)
                }),
            )
            .on_action(cx.listener(|app, _: &MonthView, window, cx| {
                app.change_view(View::Month, window, cx)
            }))
            .on_action(cx.listener(|app, _: &AgendaView, window, cx| {
                app.change_view(View::Agenda, window, cx)
            }))
            .on_action(cx.listener(|app, _: &TasksView, window, cx| {
                app.change_view(View::Tasks, window, cx)
            }))
            .on_action(cx.listener(|app, _: &Save, _, cx| app.save(cx)))
            .on_action(cx.listener(|app, _: &Escape, window, cx| app.escape(window, cx)))
            .on_action(cx.listener(|app, _: &Help, _, cx| {
                if app.editor.is_none() {
                    app.help = !app.help;
                    app.settings = false;
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|app, _: &Search, window, cx| {
                if app.editor.is_none() {
                    app.search.update(cx, |s, cx| s.focus(window, cx));
                }
            }))
            .on_action(cx.listener(|app, _: &Sync, _, _cx| app.dispatch(Command::Sync(app.date))))
            .on_action(cx.listener(|app, _: &ZoomIn, _, cx| {
                app.zoom = (app.zoom + 0.1).min(1.8);
                app.reveal_cursor = true;
                cx.notify();
            }))
            .on_action(cx.listener(|app, _: &ZoomOut, _, cx| {
                app.zoom = (app.zoom - 0.1).max(0.7);
                app.reveal_cursor = true;
                cx.notify();
            }))
            .on_action(cx.listener(|app, _: &ZoomReset, _, cx| {
                app.zoom = 1.;
                app.reveal_cursor = true;
                cx.notify();
            }))
            .on_action(cx.listener(|app, _: &ToggleSidebar, _, cx| {
                if app.editor.is_none() {
                    app.sidebar_open = !app.sidebar_open;
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|app, _: &ShowActions, window, cx| {
                if app.editor.is_none() {
                    if app.selected.is_none()
                        && let Some(e) = app.event_at_cursor(cx)
                    {
                        app.pick_event(e, window, cx);
                    }
                    if app.selected.is_some() {
                        app.event_menu = true;
                    } else {
                        app.slot_menu = true;
                    }
                    cx.notify();
                }
            }))
            .on_action(
                cx.listener(|app, _: &gpui_kit::base::actions::Confirm, window, cx| {
                    if app.focus.is_focused(window)
                        && app.editor.is_none()
                        && !app.settings
                        && !app.help
                    {
                        if let Some(e) = app.event_at_cursor(cx) {
                            app.pick_event(e, window, cx);
                        } else if app.selected.is_none() {
                            app.new_event(false, window, cx);
                        }
                    }
                }),
            )
            .on_key_down(
                cx.listener(|app, event: &gpui_kit::KeyDownEvent, window, cx| {
                    if !app.focus.is_focused(window)
                        || app.editor.is_some()
                        || app.settings
                        || app.help
                        || event.keystroke.modifiers.control
                        || event.keystroke.modifiers.platform
                    {
                        return;
                    }
                    let extend = event.keystroke.modifiers.shift;
                    let (days, minutes) = match event.keystroke.key.as_str() {
                        "left" => (-1, 0),
                        "right" => (1, 0),
                        "up" => {
                            if app.view == View::Month {
                                (-7, 0)
                            } else {
                                (0, -15)
                            }
                        }
                        "down" => {
                            if app.view == View::Month {
                                (7, 0)
                            } else {
                                (0, 15)
                            }
                        }
                        _ => return,
                    };
                    let before = app.date;
                    app.selection.move_by(days, minutes, extend);
                    app.date = app.selection.cursor.day;
                    app.selected = None;
                    app.reveal_cursor = true;
                    if app.date.month() != before.month() || app.date.year() != before.year() {
                        app.dispatch(Command::Sync(app.date));
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(toolbar)
            .child(body)
            .child(
                div()
                    .min_h(rems(1.5))
                    .flex_shrink_0()
                    .px(rems(1.))
                    .py(rems(0.3))
                    .border_t_1()
                    .border_color(t.divider())
                    .bg(t.inset)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(rems(0.5))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .text_size(rems(0.65))
                            .text_color(if self.error { t.danger } else { t.secondary })
                            .child(self.status.clone()),
                    )
                    .children((!compact).then(|| {
                        div()
                            .text_color(t.secondary)
                            .text_size(rems(0.65))
                            .child(format!(
                                "{}   ·   Shift arrows select   ·   Enter {}",
                                self.selection.label(),
                                if cursor_event.is_some() {
                                    "open"
                                } else {
                                    "add"
                                }
                            ))
                    })),
            );
        if narrow && self.sidebar_open {
            root = root.child(
                div()
                    .absolute()
                    .left_0()
                    .top(rems(3.2))
                    .bottom(rems(1.5))
                    .occlude()
                    .shadow_lg()
                    .child(self.sidebar(window, cx)),
            );
        }
        if self.selected.is_some() {
            root = root.child(self.details(window, cx));
        }
        if self.editor.is_some() {
            root = root.child(self.editor_view(window, cx));
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{Modifiers, TestAppContext, VisualTestContext, size};

    fn fixture(
        cx: &mut TestAppContext,
    ) -> (
        Entity<Ocal>,
        &mut VisualTestContext,
        mpsc::Receiver<Command>,
    ) {
        cx.update(|cx| {
            gpui_omarchy::init(cx);
            bind_keys(cx);
        });
        let date = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        let store = ocal_core::Store::memory().unwrap();
        ocal_core::demo::seed(&store, date).unwrap();
        let snapshot = store.snapshot().unwrap();
        let (commands, receiver) = mpsc::channel();
        let (_events, output) = async_channel::unbounded();
        let (view, cx) = cx.add_window_view(move |window, cx| {
            let mut app = Ocal::new(
                Worker {
                    commands,
                    events: output,
                    cancel_sign_in: Arc::new(AtomicBool::new(false)),
                },
                true,
                window,
                cx,
            );
            app.date = date;
            app.display_zone = "Europe/London".into();
            app.selection = Selection::new(date);
            app.event(WorkerEvent::Snapshot(snapshot), cx);
            app
        });
        cx.simulate_resize(size(px(1440.), px(940.)));
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        (view, cx, receiver)
    }
    fn draw(cx: &mut VisualTestContext) {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
    }

    #[gpui_kit::test]
    fn custom_sign_in_is_disclosed_and_stays_busy_until_completion(cx: &mut TestAppContext) {
        let (view, cx, commands) = fixture(cx);
        view.update(cx, |app, cx| {
            app.demo = false;
            app.settings = true;
            cx.notify();
        });
        draw(cx);
        assert!(cx.debug_bounds("connect-google-custom").is_none());
        let advanced = cx.debug_bounds("advanced-login").unwrap().center();
        cx.simulate_click(advanced, Default::default());
        draw(cx);
        let custom = cx.debug_bounds("connect-google-custom").unwrap().center();
        cx.simulate_click(custom, Default::default());
        draw(cx);
        assert!(matches!(
            commands.try_recv().unwrap(),
            Command::ConnectCustom
        ));
        view.update(cx, |app, cx| {
            app.event(
                WorkerEvent::Progress("Finish sign-in in your browser…".into()),
                cx,
            );
        });
        draw(cx);
        assert!(view.read_with(cx, |app, _| app.busy && !app.error));
        let custom = cx.debug_bounds("connect-google-custom").unwrap().center();
        cx.simulate_click(custom, Default::default());
        assert!(commands.try_recv().is_err());
        view.update(cx, |app, cx| {
            app.event(WorkerEvent::Error("Sign-in cancelled".into()), cx);
        });
        draw(cx);
        let custom = cx.debug_bounds("connect-google-custom").unwrap().center();
        cx.simulate_click(custom, Default::default());
        assert!(matches!(
            commands.try_recv().unwrap(),
            Command::ConnectCustom
        ));
        view.update(cx, |app, cx| {
            app.event(WorkerEvent::Status("Google Calendar connected".into()), cx);
        });
        assert!(view.read_with(cx, |app, _| !app.busy && !app.error));
    }

    #[gpui_kit::test]
    fn ordinary_sign_in_uses_the_publisher_client_without_a_file_picker(cx: &mut TestAppContext) {
        let (view, cx, commands) = fixture(cx);
        view.update(cx, |app, cx| {
            app.demo = false;
            app.settings = true;
            cx.notify();
        });
        draw(cx);
        let sign_in = cx.debug_bounds("connect-google").unwrap().center();
        cx.simulate_click(sign_in, Default::default());
        if ocal_core::oauth::ClientConfig::bundled().is_ok() {
            assert!(matches!(commands.try_recv().unwrap(), Command::Connect));
            assert!(view.read_with(cx, |app, _| app.busy));
        } else {
            assert!(commands.try_recv().is_err());
            assert!(view.read_with(cx, |app, _| !app.busy));
        }
        assert!(cx.debug_bounds("connect-google-custom").is_none());
    }

    #[gpui_kit::test]
    fn sign_in_errors_are_visible_at_the_button_and_success_returns_to_the_calendar(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, _) = fixture(cx);
        view.update(cx, |app, cx| {
            app.settings = true;
            app.busy = true;
            app.event(
                WorkerEvent::Progress("Google approved access · Loading calendars…".into()),
                cx,
            );
        });
        draw(cx);
        assert!(cx.debug_bounds("account-status").is_some());
        view.update(cx, |app, cx| {
            app.event(
                WorkerEvent::Error("Google Calendar API is disabled for this app".into()),
                cx,
            );
        });
        draw(cx);
        let status = cx.debug_bounds("account-status").unwrap();
        let button = cx.debug_bounds("connect-google").unwrap();
        assert!(status.top() > button.bottom());
        assert!(view.read_with(cx, |app, _| app.settings && app.error && !app.busy));
        view.update(cx, |app, cx| {
            app.active_set = Some("previous-local-set".into());
            app.event(WorkerEvent::Connected, cx);
        });
        draw(cx);
        assert!(view.read_with(cx, |app, _| !app.settings
            && !app.busy
            && !app.error
            && app.active_set.is_none()));
        assert!(cx.debug_bounds("account-status").is_none());
    }

    #[gpui_kit::test]
    fn multiple_accounts_are_grouped_and_the_editor_saves_to_the_chosen_account(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, commands) = fixture(cx);
        view.update(cx, |app, cx| {
            app.demo = false;
            for id in ["first@example.com", "second@example.com"] {
                app.snapshot.accounts.push(ocal_core::Account {
                    id: id.into(),
                    name: id.into(),
                });
                let mut calendar = app.snapshot.calendars[0].clone();
                calendar.id = format!("google:{id}:shared@example.com");
                calendar.account = Some(id.into());
                calendar.remote_id = Some("shared@example.com".into());
                calendar.name = "Work".into();
                app.snapshot.calendars.push(calendar);
            }
            let groups = calendar_groups(&app.snapshot);
            assert_eq!(groups.len(), 3);
            assert_eq!(groups[1].0.as_deref(), Some("first@example.com"));
            assert_eq!(groups[2].0.as_deref(), Some("second@example.com"));
            assert_eq!(groups[1].1, vec![3]);
            assert_eq!(groups[2].1, vec![4]);
            cx.notify();
        });
        draw(cx);
        assert!(cx.debug_bounds("calendar-group-1").is_some());
        assert!(cx.debug_bounds("calendar-group-2").is_some());
        let second_row = cx.debug_bounds("calendar-row-4").unwrap().center();
        cx.simulate_click(second_row, Default::default());
        let Command::Visible(changed) = commands.try_recv().unwrap() else {
            panic!("Expected calendar visibility");
        };
        assert_eq!(changed.account.as_deref(), Some("second@example.com"));
        assert!(!changed.visible);
        assert!(view.read_with(cx, |app, _| app.snapshot.calendars[3].visible));
        cx.simulate_keystrokes("c");
        draw(cx);
        cx.simulate_keystrokes("R e v i e w");
        view.update(cx, |app, cx| {
            let editor = app.editor.as_ref().unwrap();
            editor
                .calendar
                .update(cx, |state, cx| state.set_selected(Some(4), cx));
            let state = editor.calendar.read(cx);
            assert_eq!(
                state.selected().unwrap().label.as_ref(),
                "Work · second@example.com"
            );
        });
        cx.simulate_keystrokes("ctrl-enter");
        let Command::Save { event, .. } = commands.try_recv().unwrap() else {
            panic!("Expected saved event");
        };
        assert_eq!(
            event.calendar,
            "google:second@example.com:shared@example.com"
        );
        view.update(cx, |app, cx| {
            app.event(WorkerEvent::Saved(Box::new(event)), cx);
            app.selected = None;
            app.settings = true;
            cx.notify();
        });
        draw(cx);
        assert!(cx.debug_bounds("google-account-0").is_some());
        assert!(cx.debug_bounds("google-account-1").is_some());
        let add = cx.debug_bounds("connect-google").unwrap().center();
        cx.simulate_click(add, Default::default());
        if ocal_core::oauth::ClientConfig::bundled().is_ok() {
            assert!(matches!(commands.try_recv().unwrap(), Command::Connect));
        } else {
            assert!(commands.try_recv().is_err());
        }
        assert_eq!(view.read_with(cx, |app, _| app.snapshot.accounts.len()), 2);
        view.update(cx, |app, cx| {
            app.event(WorkerEvent::Status("Ready".into()), cx);
        });
        cx.simulate_resize(size(px(480.), px(480.)));
        draw(cx);
        for selector in ["google-account-0", "google-account-1", "connect-google"] {
            let bounds = cx.debug_bounds(selector).unwrap();
            assert!(bounds.left() >= px(0.) && bounds.right() <= px(480.));
        }
    }

    #[gpui_kit::test]
    fn sign_in_copies_the_original_link_and_copying_again_keeps_the_same_attempt(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, commands) = fixture(cx);
        let url =
            "https://accounts.google.com/o/oauth2/v2/auth?state=current&code_challenge=challenge";
        view.update(cx, |app, cx| {
            app.demo = false;
            app.settings = true;
            app.event(WorkerEvent::SignInReady(url.into()), cx);
        });
        draw(cx);
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .unwrap(),
            url
        );
        assert!(view.read_with(cx, |app, _| app.busy
            && app.sign_in_url.as_deref() == Some(url)));
        cx.write_to_clipboard(ClipboardItem::new_string("Something else".into()));
        let copy = cx.debug_bounds("copy-sign-in-link").unwrap().center();
        cx.simulate_click(copy, Default::default());
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .unwrap(),
            url
        );
        assert!(commands.try_recv().is_err());
        view.update(cx, |app, cx| {
            app.event(
                WorkerEvent::Progress("Google approved access · Loading calendars…".into()),
                cx,
            );
        });
        draw(cx);
        assert!(cx.debug_bounds("copy-sign-in-link").is_none());
        assert!(view.read_with(cx, |app, _| app.sign_in_url.is_none() && app.busy));
    }

    #[gpui_kit::test]
    fn copied_sign_in_can_be_cancelled_and_an_expired_link_is_not_reused(cx: &mut TestAppContext) {
        let (view, cx, commands) = fixture(cx);
        view.update(cx, |app, cx| {
            app.demo = false;
            app.settings = true;
            app.event(
                WorkerEvent::SignInReady(
                    "https://accounts.google.com/o/oauth2/v2/auth?state=old".into(),
                ),
                cx,
            );
        });
        draw(cx);
        let cancel = cx.debug_bounds("cancel-sign-in").unwrap().center();
        cx.simulate_click(cancel, Default::default());
        assert!(
            view.read_with(cx, |app, _| app.cancel_sign_in.load(Ordering::Relaxed)
                && app.sign_in_url.is_none())
        );
        view.update(cx, |app, cx| {
            app.event(WorkerEvent::Status("Sign-in cancelled".into()), cx);
        });
        draw(cx);
        let start = cx.debug_bounds("connect-google").unwrap().center();
        cx.simulate_click(start, Default::default());
        if ocal_core::oauth::ClientConfig::bundled().is_ok() {
            assert!(matches!(commands.try_recv().unwrap(), Command::Connect));
            assert!(view.read_with(cx, |app, _| !app.cancel_sign_in.load(Ordering::Relaxed)));
        }
        view.update(cx, |app, cx| {
            app.event(
                WorkerEvent::SignInReady(
                    "https://accounts.google.com/o/oauth2/v2/auth?state=fresh".into(),
                ),
                cx,
            );
        });
        assert!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .unwrap()
                .contains("state=fresh")
        );
        view.update(cx, |app, cx| {
            app.event(
                WorkerEvent::Error("Sign-in link expired. Copy a fresh link".into()),
                cx,
            );
        });
        draw(cx);
        assert!(view.read_with(cx, |app, _| !app.busy && app.sign_in_url.is_none()));
        assert!(cx.debug_bounds("copy-sign-in-link").is_none());
    }

    #[gpui_kit::test]
    fn demo_disables_both_google_sign_in_routes(cx: &mut TestAppContext) {
        let (view, cx, commands) = fixture(cx);
        view.update(cx, |app, cx| {
            app.settings = true;
            app.advanced_login = true;
            cx.notify();
        });
        draw(cx);
        for selector in ["connect-google", "connect-google-custom"] {
            let button = cx.debug_bounds(selector).unwrap().center();
            cx.simulate_click(button, Default::default());
        }
        assert!(commands.try_recv().is_err());
        assert!(view.read_with(cx, |app, _| !app.busy));
    }

    #[gpui_kit::test]
    fn keyboard_selects_quarters_and_typing_stays_in_the_editor(cx: &mut TestAppContext) {
        let (view, cx, commands) = fixture(cx);
        cx.simulate_keystrokes("shift-down shift-down enter");
        draw(cx);
        view.read_with(cx, |app, cx| {
            let f = app.editor.as_ref().unwrap();
            assert_eq!(f.fields[2].read(cx).value().as_ref(), "09:00");
            assert_eq!(f.fields[4].read(cx).value().as_ref(), "09:30");
        });
        cx.simulate_keystrokes("c t b 1");
        draw(cx);
        view.read_with(cx, |app, cx| {
            assert_eq!(
                app.editor.as_ref().unwrap().fields[0]
                    .read(cx)
                    .value()
                    .as_ref(),
                "ctb1"
            );
            assert_eq!(app.view, View::Week);
            assert_eq!(app.selection.cursor.minute, 9 * 60 + 30);
        });
        cx.simulate_keystrokes("ctrl-enter");
        draw(cx);
        let saved = commands
            .try_iter()
            .find_map(|c| {
                if let Command::Save { event, .. } = c {
                    Some(event)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(saved.end - saved.start, Duration::minutes(30));
        assert!(view.read_with(cx, |app, _| app.busy));
        cx.simulate_keystrokes("x");
        draw(cx);
        assert_eq!(
            view.read_with(cx, |app, cx| app.editor.as_ref().unwrap().fields[0]
                .read(cx)
                .value()
                .to_string()),
            "ctb1"
        );
        view.update(cx, |app, cx| {
            app.event(WorkerEvent::Error("Offline".into()), cx)
        });
        draw(cx);
        cx.simulate_keystrokes("x");
        draw(cx);
        assert_eq!(
            view.read_with(cx, |app, cx| app.editor.as_ref().unwrap().fields[0]
                .read(cx)
                .value()
                .to_string()),
            "ctb1x"
        );
        view.update(cx, |app, cx| {
            app.event(WorkerEvent::Saved(Box::new(saved)), cx)
        });
        draw(cx);
        cx.simulate_keystrokes("escape down");
        draw(cx);
        assert!(view.read_with(cx, |app, _| app.editor.is_none()
            && app.selection.cursor.minute == 9 * 60 + 45));
    }

    #[gpui_kit::test]
    fn timed_event_bounds_match_duration_even_for_short_and_overnight_events(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, _) = fixture(cx);
        cx.simulate_resize(size(px(1920.), px(1100.)));
        view.update(cx, |app, cx| {
            app.snapshot.events.clear();
            for (hour, minutes) in [(9, 5), (10, 15), (11, 30), (12, 60), (14, 90), (23, 120)] {
                let mut event = Event::new("local-work", app.date, &app.display_zone).unwrap();
                event.title = format!("{minutes}-minute event with a long title");
                event.start =
                    local_datetime(app.date.and_hms_opt(hour, 0, 0).unwrap(), &app.display_zone)
                        .unwrap();
                event.end = event.start + Duration::minutes(minutes);
                app.snapshot.events.push(event);
            }
            cx.notify();
        });
        for zoom in [0.7, 1., 1.8] {
            view.update(cx, |app, cx| {
                app.zoom = zoom;
                cx.notify();
            });
            draw(cx);
            let day = view.read_with(cx, |app, _| app.grid_bounds[&app.date]);
            for (index, (hour, minutes)) in [
                (9., 5.),
                (10., 15.),
                (11., 30.),
                (12., 60.),
                (14., 90.),
                (23., 60.),
            ]
            .into_iter()
            .enumerate()
            {
                let event = cx
                    .debug_bounds(
                        [
                            "grid-event-0",
                            "grid-event-1",
                            "grid-event-2",
                            "grid-event-3",
                            "grid-event-4",
                            "grid-event-5",
                        ][index],
                    )
                    .unwrap();
                assert!(
                    (event.size.height - day.size.height * minutes / 1440.).abs() < px(1.),
                    "duration {minutes}, zoom {zoom}: {event:?}"
                );
                assert!((event.top() - (day.top() + day.size.height * hour / 24.)).abs() < px(1.));
            }
            view.update(cx, |app, cx| {
                app.grid_scroll.set_offset(point(px(0.), px(0.)));
                app.reveal_cursor = false;
                cx.notify();
            });
            draw(cx);
            let next = view.read_with(cx, |app, _| {
                app.grid_bounds[&(app.date + Duration::days(1))]
            });
            let continuation = cx.debug_bounds("grid-event-1000").unwrap();
            assert!((continuation.top() - next.top()).abs() < px(1.));
            assert!((continuation.size.height - next.size.height / 24.).abs() < px(1.));
        }
    }

    #[gpui_kit::test]
    fn time_line_highlights_events_and_enter_opens_them(cx: &mut TestAppContext) {
        let (view, cx, commands) = fixture(cx);
        assert_eq!(cx.debug_bounds("time-cursor").unwrap().size.height, px(2.));
        assert!(cx.debug_bounds("time-selection-0").is_none());
        assert!(cx.debug_bounds("highlighted-event-0").is_some());
        cx.simulate_keystrokes("up");
        draw(cx);
        assert!(cx.debug_bounds("highlighted-event-0").is_none());
        cx.simulate_keystrokes("down enter");
        draw(cx);
        view.read_with(cx, |app, _| {
            assert_eq!(app.selected.as_ref().unwrap().title, "Weekly planning");
            assert!(app.editor.is_none());
        });
        assert!(commands.try_recv().is_err());
        cx.simulate_keystrokes("escape down down");
        draw(cx);
        // End times are exclusive: 09:30 is free after the 09:00–09:30 event.
        assert!(view.read_with(cx, |app, cx| app.event_at_cursor(cx).is_none()));
        cx.simulate_keystrokes("enter");
        draw(cx);
        view.read_with(cx, |app, cx| {
            let f = app.editor.as_ref().unwrap();
            assert!(!f.editing);
            assert_eq!(f.fields[2].read(cx).value().as_ref(), "09:30");
            assert_eq!(f.fields[4].read(cx).value().as_ref(), "09:45");
        });
    }

    #[gpui_kit::test]
    fn shift_navigation_creates_through_events_and_backtracks_to_the_line(cx: &mut TestAppContext) {
        let (view, cx, _) = fixture(cx);
        cx.simulate_keystrokes("shift-down");
        draw(cx);
        let range = cx.debug_bounds("time-selection-0").unwrap();
        let day = view.read_with(cx, |app, _| app.grid_bounds[&app.date]);
        assert!((range.size.height - day.size.height / 96.).abs() < px(1.));
        assert!(cx.debug_bounds("highlighted-event-0").is_none());
        assert!(view.read_with(cx, |app, cx| app.event_at_cursor(cx).is_none()));
        cx.simulate_keystrokes("shift-up");
        draw(cx);
        assert!(cx.debug_bounds("time-selection-0").is_none());
        assert!(cx.debug_bounds("highlighted-event-0").is_none());
        cx.simulate_keystrokes("shift-up enter");
        draw(cx);
        view.read_with(cx, |app, cx| {
            let f = app.editor.as_ref().unwrap();
            assert!(!f.editing && app.selected.is_none());
            assert_eq!(f.fields[2].read(cx).value().as_ref(), "08:45");
            assert_eq!(f.fields[4].read(cx).value().as_ref(), "09:00");
        });
        cx.simulate_keystrokes("escape shift-down shift-down enter");
        draw(cx);
        view.read_with(cx, |app, cx| {
            let f = app.editor.as_ref().unwrap();
            assert_eq!(f.fields[2].read(cx).value().as_ref(), "08:45");
            assert_eq!(f.fields[4].read(cx).value().as_ref(), "09:15");
        });
    }

    #[gpui_kit::test]
    fn cursor_prefers_the_newly_crossed_overlap_and_respects_calendar_filters(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, _) = fixture(cx);
        cx.simulate_keystrokes("down down down down enter");
        draw(cx);
        assert_eq!(
            view.read_with(cx, |app, _| app.selected.as_ref().unwrap().title.clone()),
            "Deep work"
        );
        cx.simulate_keystrokes("escape down down enter");
        draw(cx);
        assert_eq!(
            view.read_with(cx, |app, _| app.selected.as_ref().unwrap().title.clone()),
            "Design review"
        );
        cx.simulate_keystrokes("escape down down enter");
        draw(cx);
        assert_eq!(
            view.read_with(cx, |app, _| app.selected.as_ref().unwrap().title.clone()),
            "Deep work"
        );
        cx.simulate_keystrokes("escape");
        view.update(cx, |app, cx| {
            app.snapshot
                .calendars
                .iter_mut()
                .find(|c| c.id == "local-work")
                .unwrap()
                .visible = false;
            cx.notify();
        });
        draw(cx);
        assert!(view.read_with(cx, |app, cx| app.event_at_cursor(cx).is_none()));
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert!(view.read_with(cx, |app, _| app.editor.is_some() && app.selected.is_none()));
    }

    #[gpui_kit::test]
    fn highlighted_events_support_keyboard_context_actions(cx: &mut TestAppContext) {
        let (view, cx, _) = fixture(cx);
        cx.simulate_keystrokes("shift-f10");
        draw(cx);
        assert!(cx.debug_bounds("omarchy-menu-content").is_some());
        assert!(view.read_with(cx, |app, _| app.event_menu && !app.slot_menu));
        cx.simulate_keystrokes("down enter");
        draw(cx);
        view.read_with(cx, |app, _| {
            let f = app.editor.as_ref().unwrap();
            assert!(f.editing);
            assert_eq!(f.base.title, "Weekly planning");
        });
    }

    #[gpui_kit::test]
    fn shift_drag_snaps_to_quarters_and_opens_the_same_editor(cx: &mut TestAppContext) {
        let (view, cx, _commands) = fixture(cx);
        // Thursday 12:30 is empty, after the dentist and before focus time.
        view.update(cx, |app, cx| {
            app.selection.cursor.minute = 12 * 60;
            app.reveal_cursor = true;
            cx.notify();
        });
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        let date = NaiveDate::from_ymd_opt(2026, 10, 8).unwrap();
        let b = view.read_with(cx, |app, _| app.grid_bounds[&date]);
        let from = point(
            b.center().x,
            b.top() + b.size.height * (12. * 60. + 32.) / 1440.,
        );
        let to = point(
            b.center().x,
            b.top() + b.size.height * (13. * 60. + 2.) / 1440.,
        );
        assert!(cx.debug_bounds("time-scroll").unwrap().contains(&from));
        let shift = Modifiers {
            shift: true,
            ..Default::default()
        };
        cx.simulate_mouse_down(from, MouseButton::Left, shift);
        assert!(
            view.read_with(cx, |app, _| app.dragging),
            "from={from:?} b={b:?} scroll={:?}",
            cx.debug_bounds("time-scroll")
        );
        cx.simulate_mouse_move(to, Some(MouseButton::Left), shift);
        cx.simulate_mouse_up(to, MouseButton::Left, shift);
        draw(cx);
        view.read_with(cx, |app, cx| {
            let f = app.editor.as_ref().expect("drag opens editor");
            assert_eq!(f.fields[1].read(cx).value().as_ref(), "2026-10-08");
            assert_eq!(f.fields[2].read(cx).value().as_ref(), "12:30");
            assert_eq!(f.fields[4].read(cx).value().as_ref(), "13:15");
        });
    }

    #[gpui_kit::test]
    fn context_menu_owns_navigation_and_escape_does_not_reopen_it(cx: &mut TestAppContext) {
        let (view, cx, _commands) = fixture(cx);
        cx.simulate_keystrokes("down down shift-f10");
        draw(cx);
        assert!(cx.debug_bounds("omarchy-menu-content").is_some());
        let slot = view.read_with(cx, |app, _| app.selection.cursor);
        cx.simulate_keystrokes("down");
        draw(cx);
        assert_eq!(view.read_with(cx, |app, _| app.selection.cursor), slot);
        cx.simulate_keystrokes("escape");
        draw(cx);
        assert!(cx.debug_bounds("omarchy-menu-content").is_none());
        assert!(!view.read_with(cx, |app, _| app.slot_menu));
        cx.simulate_keystrokes("shift-f10 enter");
        draw(cx);
        assert!(view.read_with(cx, |app, _| app.editor.is_some()));
    }

    #[gpui_kit::test]
    fn right_click_opens_at_the_clicked_slot_and_uses_its_time(cx: &mut TestAppContext) {
        let (view, cx, _commands) = fixture(cx);
        let date = NaiveDate::from_ymd_opt(2026, 10, 8).unwrap();
        let b = view.read_with(cx, |app, _| app.grid_bounds[&date]);
        let at = point(
            b.center().x,
            b.top() + b.size.height * (12. * 60. + 32.) / 1440.,
        );
        cx.simulate_mouse_down(at, MouseButton::Right, Default::default());
        cx.simulate_mouse_up(at, MouseButton::Right, Default::default());
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        let menu = cx.debug_bounds("omarchy-menu-content").unwrap();
        assert!((menu.top() - at.y).abs() < px(5.));
        cx.simulate_keystrokes("enter");
        draw(cx);
        view.read_with(cx, |app, cx| {
            let f = app.editor.as_ref().unwrap();
            assert_eq!(f.fields[1].read(cx).value().as_ref(), "2026-10-08");
            assert_eq!(f.fields[2].read(cx).value().as_ref(), "12:30");
        });
    }

    #[gpui_kit::test]
    fn calendar_columns_fill_the_available_area(cx: &mut TestAppContext) {
        let (view, cx, _commands) = fixture(cx);
        let left = cx.debug_bounds("grid-day-0").unwrap();
        let right = cx.debug_bounds("grid-day-6").unwrap();
        assert!(left.size.width > px(150.));
        assert!((right.right() - px(1440.)).abs() < px(2.));
        assert!(view.read_with(cx, |app, _| app.grid_scroll.offset().y < px(-300.)));
        cx.simulate_keystrokes("3");
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        let first = cx.debug_bounds("month-day-0").unwrap();
        let last = cx.debug_bounds("month-day-41").unwrap();
        assert!(first.size.width > px(150.) && first.size.height > px(100.));
        assert!((last.right() - px(1440.)).abs() < px(2.));
        assert!(last.bottom() > px(890.) && last.bottom() <= px(940.));
    }

    #[gpui_kit::test]
    fn pointer_hits_resolve_to_the_visible_week_after_navigation(cx: &mut TestAppContext) {
        let (view, cx, _commands) = fixture(cx);
        cx.simulate_keystrokes("]");
        draw(cx);
        let next_monday = NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
        view.read_with(cx, |app, _| {
            assert_eq!(app.grid_bounds.len(), 7);
            let b = app.grid_bounds[&next_monday];
            let slot = app.pointer_slot(b.center()).unwrap();
            assert_eq!(slot.day, next_monday);
        });
    }

    #[gpui_kit::test]
    fn half_width_tiles_keep_the_week_and_view_buttons(cx: &mut TestAppContext) {
        let (_view, cx, _commands) = fixture(cx);
        // A half-width Hyprland tile on a 1728-pixel-wide logical display.
        cx.simulate_resize(size(px(850.), px(1040.)));
        draw(cx);
        assert!(cx.debug_bounds("grid-day-6").is_some());
        assert!(cx.debug_bounds("tool-Calendars").is_some());
        let right = cx.debug_bounds("grid-day-6").unwrap();
        assert!(right.right() <= px(851.));
        cx.simulate_resize(size(px(600.), px(800.)));
        draw(cx);
        assert!(cx.debug_bounds("grid-day-1").is_none());
    }

    #[gpui_kit::test]
    fn meeting_choices_are_explicit_and_retries_keep_the_same_request(cx: &mut TestAppContext) {
        let (view, cx, commands) = fixture(cx);
        view.update(cx, |app, cx| {
            let mut calendar = app.snapshot.calendars[0].clone();
            calendar.id = "google-work".into();
            calendar.account = Some("a".into());
            calendar.remote_id = Some("primary".into());
            calendar.meet_available = true;
            app.snapshot.calendars.insert(0, calendar);
            cx.notify();
        });
        cx.simulate_keystrokes("c");
        draw(cx);
        cx.simulate_keystrokes("R e v i e w");
        draw(cx);
        assert!(view.read_with(cx, |app, cx| app.draft(cx).unwrap().meet_request.is_none()));
        let b = cx.debug_bounds("meeting-trigger").unwrap();
        cx.simulate_click(b.center(), Default::default());
        draw(cx);
        cx.simulate_keystrokes("end enter");
        draw(cx);
        assert_eq!(
            view.read_with(cx, |app, _| app.editor.as_ref().unwrap().meeting),
            2
        );
        cx.simulate_keystrokes("ctrl-enter");
        draw(cx);
        let first = commands
            .try_iter()
            .find_map(|c| {
                if let Command::Save { event, .. } = c {
                    Some(event)
                } else {
                    None
                }
            })
            .unwrap();
        assert!(first.meet_request.is_some());
        view.update(cx, |app, cx| {
            app.event(WorkerEvent::Error("Retry".into()), cx)
        });
        draw(cx);
        cx.simulate_keystrokes("ctrl-enter");
        draw(cx);
        let retry = commands
            .try_iter()
            .find_map(|c| {
                if let Command::Save { event, .. } = c {
                    Some(event)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(first.meet_request, retry.meet_request);
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.event(WorkerEvent::Error("Retry".into()), cx);
                let f = app.editor.as_mut().unwrap();
                f.meeting = 1;
                f.fields[6].update(cx, |s, cx| {
                    s.set_value("https://example.com/meeting", window, cx)
                });
                cx.notify();
            })
        });
        draw(cx);
        view.read_with(cx, |app, cx| {
            let e = app.draft(cx).unwrap();
            assert!(e.meet_request.is_none());
            assert_eq!(e.link, "https://example.com/meeting");
        });
    }

    #[gpui_kit::test]
    fn paste_link_focus_and_explicit_removal_preserve_other_notes(cx: &mut TestAppContext) {
        let (view, cx, _commands) = fixture(cx);
        cx.simulate_keystrokes("c");
        draw(cx);
        cx.simulate_keystrokes("M e e t");
        draw(cx);
        let trigger = cx.debug_bounds("meeting-trigger").unwrap().center();
        cx.simulate_click(trigger, Default::default());
        draw(cx);
        cx.simulate_keystrokes("home down enter");
        draw(cx);
        cx.update(|window, cx| {
            view.read_with(cx, |app, cx| {
                assert!(
                    app.editor.as_ref().unwrap().fields[6]
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)
                );
            })
        });
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                let f = app.editor.as_mut().unwrap();
                f.base.has_conference = true;
                f.base.link = "https://meet.google.com/abc-defg-hij".into();
                f.notes.update(cx, |s, cx| {
                    s.set_value(
                        "Keep these notes\n\nhttps://meet.google.com/abc-defg-hij",
                        window,
                        cx,
                    )
                });
                f.meeting = 0;
                cx.notify();
            })
        });
        draw(cx);
        view.read_with(cx, |app, cx| {
            let e = app.draft(cx).unwrap();
            assert!(e.clear_conference);
            assert!(e.link.is_empty() && e.meet_request.is_none());
            assert_eq!(e.notes, "Keep these notes");
        });
    }

    #[gpui_kit::test]
    fn compact_context_menu_stays_inside_the_window(cx: &mut TestAppContext) {
        let (view, cx, _commands) = fixture(cx);
        cx.update(|_, cx| gpui_omarchy::Theme::flexoki_light().apply(cx));
        view.update(cx, |app, cx| {
            app.zoom = 1.8;
            cx.notify();
        });
        cx.simulate_resize(size(px(480.), px(480.)));
        draw(cx);
        cx.simulate_keystrokes("shift-f10");
        draw(cx);
        let b = cx.debug_bounds("omarchy-menu-content").unwrap();
        assert!(b.left() >= px(0.) && b.right() <= px(480.));
        assert!(b.top() >= px(0.) && b.bottom() <= px(480.));
        cx.simulate_keystrokes("escape");
        draw(cx);
        assert!(cx.debug_bounds("omarchy-menu-content").is_none());
    }

    #[gpui_kit::test]
    fn editor_fits_small_light_windows_and_reveals_keyboard_focus_at_high_zoom(
        cx: &mut TestAppContext,
    ) {
        let (view, cx, _commands) = fixture(cx);
        cx.update(|_, cx| gpui_omarchy::Theme::flexoki_light().apply(cx));
        view.update(cx, |app, cx| {
            app.zoom = 1.8;
            cx.notify();
        });
        cx.simulate_resize(size(px(480.), px(480.)));
        draw(cx);
        cx.simulate_keystrokes("c");
        draw(cx);
        let popup = cx.debug_bounds("event-editor").unwrap();
        assert!(popup.left() >= px(0.) && popup.right() <= px(480.));
        assert!(popup.top() >= px(0.) && popup.bottom() <= px(480.));
        let save = cx.debug_bounds("save-event").unwrap();
        assert!(save.bottom() <= px(480.));
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                let f = app.editor.as_mut().unwrap();
                f.details = true;
                f.fields[8].update(cx, |s, cx| s.focus(window, cx));
                cx.notify();
            })
        });
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        assert!(view.read_with(cx, |app, _| app.form_scroll.offset().y < px(0.)));
        view.update(cx, |app, cx| {
            app.form_scroll.set_offset(point(px(0.), px(0.)));
            cx.notify();
        });
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        assert_eq!(
            view.read_with(cx, |app, _| app.form_scroll.offset().y),
            px(0.)
        );
    }
}
