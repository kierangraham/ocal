//! Settings and keyboard help pages.
use super::*;

impl Ocal {
    pub(super) fn settings_view(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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
    pub(super) fn help_view(&self, cx: &mut Context<Self>) -> AnyElement {
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
