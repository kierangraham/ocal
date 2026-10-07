mod agenda;
mod details;
mod editor;
mod grid;
mod month;
mod settings;
mod sidebar;
#[cfg(test)]
mod tests;

use crate::{
    appearance::{action_button, day_number, event_text, reveal_field, tint, tool_button},
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
    pub(super) fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn pick_event(&mut self, e: Event, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.is_some() {
            return;
        }
        self.selected = Some(e);
        self.delete_confirm = false;
        window.focus(&self.focus, cx);
        cx.notify();
    }
    pub(super) fn duplicate(&mut self, e: &Event, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn event_action(
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
    pub(super) fn event_items(&self, e: &Event) -> Vec<MenuItem> {
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
