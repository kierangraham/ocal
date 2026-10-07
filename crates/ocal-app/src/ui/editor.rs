//! Event editor.
use super::*;

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
    pub(super) fn draft(&self, cx: &App) -> anyhow::Result<Event> {
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
    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
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
    pub(super) fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn editor_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
}
pub(super) fn row(label: &'static str, content: AnyElement, cx: &App) -> AnyElement {
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
