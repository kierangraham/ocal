use super::{sidebar::calendar_groups, *};
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
    let url = "https://accounts.google.com/o/oauth2/v2/auth?state=current&code_challenge=challenge";
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
        assert_eq!(f.start_time.read(cx).value().as_ref(), "09:00");
        assert_eq!(f.end_time.read(cx).value().as_ref(), "09:30");
    });
    cx.simulate_keystrokes("c t b 1");
    draw(cx);
    view.read_with(cx, |app, cx| {
        assert_eq!(
            app.editor.as_ref().unwrap().title.read(cx).value().as_ref(),
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
        view.read_with(cx, |app, cx| app
            .editor
            .as_ref()
            .unwrap()
            .title
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
        view.read_with(cx, |app, cx| app
            .editor
            .as_ref()
            .unwrap()
            .title
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
fn timed_event_bounds_match_duration_even_for_short_and_overnight_events(cx: &mut TestAppContext) {
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
                        "grid-event-1000",
                        "grid-event-1001",
                        "grid-event-1002",
                        "grid-event-1003",
                        "grid-event-1004",
                        "grid-event-1005",
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
        let continuation = cx.debug_bounds("grid-event-2000").unwrap();
        assert!((continuation.top() - next.top()).abs() < px(1.));
        assert!((continuation.size.height - next.size.height / 24.).abs() < px(1.));
    }
}

#[gpui_kit::test]
fn time_line_highlights_events_and_enter_opens_them(cx: &mut TestAppContext) {
    let (view, cx, commands) = fixture(cx);
    assert_eq!(cx.debug_bounds("time-cursor").unwrap().size.height, px(2.));
    assert!(cx.debug_bounds("time-selection-1").is_none());
    assert!(cx.debug_bounds("highlighted-event-1000").is_some());
    cx.simulate_keystrokes("up");
    draw(cx);
    assert!(cx.debug_bounds("highlighted-event-1000").is_none());
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
        assert_eq!(f.start_time.read(cx).value().as_ref(), "09:30");
        assert_eq!(f.end_time.read(cx).value().as_ref(), "09:45");
    });
}

#[gpui_kit::test]
fn shift_navigation_creates_through_events_and_backtracks_to_the_line(cx: &mut TestAppContext) {
    let (view, cx, _) = fixture(cx);
    cx.simulate_keystrokes("shift-down");
    draw(cx);
    let range = cx.debug_bounds("time-selection-1").unwrap();
    let day = view.read_with(cx, |app, _| app.grid_bounds[&app.date]);
    assert!((range.size.height - day.size.height / 96.).abs() < px(1.));
    assert!(cx.debug_bounds("highlighted-event-1000").is_none());
    assert!(view.read_with(cx, |app, cx| app.event_at_cursor(cx).is_none()));
    cx.simulate_keystrokes("shift-up");
    draw(cx);
    assert!(cx.debug_bounds("time-selection-1").is_none());
    assert!(cx.debug_bounds("highlighted-event-1000").is_none());
    cx.simulate_keystrokes("shift-up enter");
    draw(cx);
    view.read_with(cx, |app, cx| {
        let f = app.editor.as_ref().unwrap();
        assert!(!f.editing && app.selected.is_none());
        assert_eq!(f.start_time.read(cx).value().as_ref(), "08:45");
        assert_eq!(f.end_time.read(cx).value().as_ref(), "09:00");
    });
    cx.simulate_keystrokes("escape shift-down shift-down enter");
    draw(cx);
    view.read_with(cx, |app, cx| {
        let f = app.editor.as_ref().unwrap();
        assert_eq!(f.start_time.read(cx).value().as_ref(), "08:45");
        assert_eq!(f.end_time.read(cx).value().as_ref(), "09:15");
    });
}

#[gpui_kit::test]
fn cursor_prefers_the_newly_crossed_overlap_and_respects_calendar_filters(cx: &mut TestAppContext) {
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
        assert_eq!(f.start_date(cx).unwrap().to_string(), "2026-10-08");
        assert_eq!(f.start_time.read(cx).value().as_ref(), "12:30");
        assert_eq!(f.end_time.read(cx).value().as_ref(), "13:15");
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
        assert_eq!(f.start_date(cx).unwrap().to_string(), "2026-10-08");
        assert_eq!(f.start_time.read(cx).value().as_ref(), "12:30");
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
    let b = cx.debug_bounds("meeting-2").unwrap();
    cx.simulate_click(b.center(), Default::default());
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
            f.link.update(cx, |s, cx| {
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
    let trigger = cx.debug_bounds("meeting-1").unwrap().center();
    cx.simulate_click(trigger, Default::default());
    draw(cx);
    cx.update(|window, cx| {
        view.read_with(cx, |app, cx| {
            assert!(
                app.editor
                    .as_ref()
                    .unwrap()
                    .link
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
            f.notes.update(cx, |s, cx| s.focus(window, cx));
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

#[gpui_kit::test]
fn period_titles_name_both_months_when_a_week_spans_them(cx: &mut TestAppContext) {
    let (view, cx, _commands) = fixture(cx);
    view.update(cx, |app, _| {
        assert_eq!(
            app.period_title(false),
            ("October".to_string(), "2026".to_string())
        );
        app.date = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
        assert_eq!(app.period_title(false).0, "Sep – Oct");
        app.date = NaiveDate::from_ymd_opt(2026, 12, 30).unwrap();
        assert_eq!(
            app.period_title(false),
            ("Dec 2026 – Jan".to_string(), "2027".to_string())
        );
        app.view = View::Day;
        assert_eq!(app.period_title(false).0, "Wednesday 30 December");
    });
}

#[gpui_kit::test]
fn crowded_month_days_link_to_the_day_view(cx: &mut TestAppContext) {
    let (view, cx, _commands) = fixture(cx);
    let busy = NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
    view.update(cx, |app, cx| {
        for hour in 8..18 {
            let mut e = Event::new("local-work", busy, &app.display_zone).unwrap();
            e.title = format!("Meeting {hour}");
            e.start =
                local_datetime(busy.and_hms_opt(hour, 0, 0).unwrap(), &app.display_zone).unwrap();
            e.end = e.start + Duration::minutes(30);
            app.snapshot.events.push(e);
        }
        cx.notify();
    });
    cx.simulate_keystrokes("3");
    draw(cx);
    cx.run_until_parked();
    draw(cx);
    // 14 October is the 18th cell of a Sunday-first grid starting on 27 September.
    let more = cx.debug_bounds("month-more-17").unwrap();
    let cell = cx.debug_bounds("month-day-17").unwrap();
    assert!(more.bottom() <= cell.bottom() + px(1.));
    cx.simulate_click(more.center(), Default::default());
    draw(cx);
    assert!(view.read_with(cx, |app, _| app.view == View::Day && app.date == busy));
}

#[gpui_kit::test]
fn narrow_windows_open_search_from_the_header_and_escape_folds_it(cx: &mut TestAppContext) {
    let (view, cx, _commands) = fixture(cx);
    cx.simulate_resize(size(px(850.), px(900.)));
    draw(cx);
    assert!(cx.debug_bounds("search").is_none());
    assert!(cx.debug_bounds("period-title").is_some());
    cx.simulate_keystrokes("/");
    draw(cx);
    assert!(cx.debug_bounds("search").is_some());
    cx.simulate_keystrokes("C o f f e e");
    draw(cx);
    assert!(cx.debug_bounds("agenda-scroll").is_some());
    cx.simulate_keystrokes("escape escape");
    draw(cx);
    assert!(view.read_with(cx, |app, cx| !app.search_open
        && app.search.read(cx).value().is_empty()));
    assert!(cx.debug_bounds("search").is_none());
}

#[gpui_kit::test]
fn typed_and_stepped_times_carry_the_end_with_the_start(cx: &mut TestAppContext) {
    let (view, cx, commands) = fixture(cx);
    cx.simulate_keystrokes("c");
    draw(cx);
    cx.simulate_keystrokes("R u n");
    let times = |cx: &mut VisualTestContext| {
        view.read_with(cx, |app, cx| {
            let f = app.editor.as_ref().unwrap();
            (
                f.start_time.read(cx).value().to_string(),
                f.end_time.read(cx).value().to_string(),
            )
        })
    };
    assert_eq!(times(cx), ("09:00".into(), "09:15".into()));
    cx.update(|window, cx| {
        view.update(cx, |app, cx| {
            let f = app.editor.as_ref().unwrap();
            f.start_time.update(cx, |s, cx| s.focus(window, cx));
        })
    });
    draw(cx);
    cx.simulate_keystrokes("up up");
    draw(cx);
    assert_eq!(times(cx), ("09:30".into(), "09:45".into()));
    cx.simulate_keystrokes("ctrl-a backspace 2 p m");
    draw(cx);
    assert_eq!(times(cx), ("2pm".into(), "14:15".into()));
    cx.simulate_keystrokes("ctrl-enter");
    let Command::Save { event, .. } = commands
        .try_iter()
        .find(|c| matches!(c, Command::Save { .. }))
        .unwrap()
    else {
        unreachable!()
    };
    let start = event.start.with_timezone(&zone("Europe/London"));
    assert_eq!(start.format("%H:%M").to_string(), "14:00");
    assert_eq!(event.end - event.start, Duration::minutes(15));
}

#[gpui_kit::test]
fn remembered_choices_apply_once_and_changes_are_saved(cx: &mut TestAppContext) {
    let (view, cx, commands) = fixture(cx);
    view.update(cx, |app, cx| {
        app.preferences_applied = false;
        let mut snapshot = app.snapshot.clone();
        for (key, value) in [
            ("view", "month"),
            ("zoom", "1.2"),
            ("zone", "America/New_York"),
            ("sidebar", "hidden"),
            ("set", "missing-set"),
        ] {
            snapshot.preferences.insert(key.into(), value.into());
        }
        app.event(WorkerEvent::Snapshot(snapshot), cx);
    });
    view.read_with(cx, |app, cx| {
        assert_eq!(app.view, View::Month);
        assert!((app.zoom - 1.2).abs() < 0.01);
        assert_eq!(app.display_zone, "America/New_York");
        assert_eq!(
            app.zone_choice.read(cx).selected().unwrap().value.as_ref(),
            "America/New_York"
        );
        assert!(app.sidebar_hidden);
        // A set that no longer exists is ignored.
        assert!(app.active_set.is_none());
    });
    // Applying the remembered zone does not write it back.
    assert!(commands.try_recv().is_err());
    cx.simulate_keystrokes("2");
    draw(cx);
    let saved: Vec<_> = commands
        .try_iter()
        .filter_map(|c| match c {
            Command::SavePreference(k, v) => Some((k, v)),
            _ => None,
        })
        .collect();
    assert_eq!(saved, vec![("view".to_string(), "week".to_string())]);
    view.update(cx, |app, cx| {
        let mut snapshot = app.snapshot.clone();
        snapshot.preferences.insert("view".into(), "agenda".into());
        app.event(WorkerEvent::Snapshot(snapshot), cx);
    });
    assert!(view.read_with(cx, |app, _| app.view == View::Week));
}

#[gpui_kit::test]
fn settings_and_help_pages_render_and_return_to_the_calendar(cx: &mut TestAppContext) {
    let (view, cx, commands) = fixture(cx);
    view.update(cx, |app, cx| {
        app.snapshot.sets.push(CalendarSet {
            id: "focus".into(),
            name: "Focus".into(),
            calendars: vec!["local-work".into()],
        });
        app.settings = true;
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("set-0").is_some());
    let remove = cx.debug_bounds("tool-Remove set").unwrap().center();
    cx.simulate_click(remove, Default::default());
    assert!(matches!(commands.try_recv().unwrap(), Command::DeleteSet(id) if id == "focus"));
    let back = cx.debug_bounds("back-to-calendar").unwrap().center();
    cx.simulate_click(back, Default::default());
    draw(cx);
    assert!(view.read_with(cx, |app, _| !app.settings));
    cx.simulate_keystrokes("?");
    draw(cx);
    assert!(view.read_with(cx, |app, _| app.help));
    let close = cx.debug_bounds("close-help").unwrap().center();
    cx.simulate_click(close, Default::default());
    assert!(view.read_with(cx, |app, _| !app.help));
}
