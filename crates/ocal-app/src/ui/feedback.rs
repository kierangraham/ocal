//! Toasts and the shortcuts card.
use super::*;
use crate::appearance::{RADIUS_CARD, RADIUS_FIELD, Tones, soft_keycap};
use gpui_kit::{Animation, AnimationExt, Hsla, SharedString};
use std::time::Duration as Span;

/// How long an info toast stays.
const TOAST_LIFE: Span = Span::from_secs(4);
const TOAST_MOTION: Span = Span::from_millis(180);
/// Errors stay until dismissed; beyond this many the oldest goes.
const MAX_ERRORS: usize = 3;

type ToastAction = Box<dyn FnOnce(&mut Ocal, &mut Window, &mut Context<Ocal>)>;
type ToastExpiry = Box<dyn FnOnce(&mut Ocal, &mut Context<Ocal>)>;

/// A short message over the bottom of the sheet, like Omail's.
pub(super) struct Toast {
    id: usize,
    text: SharedString,
    error: bool,
    /// Stays until dismissed rather than fading after `TOAST_LIFE`.
    sticky: bool,
    action: Option<(SharedString, ToastAction)>,
    expire: Option<ToastExpiry>,
    /// Fading out before removal.
    leaving: bool,
}
impl Toast {
    pub(super) fn info(text: impl Into<SharedString>) -> Self {
        Self {
            id: 0,
            text: text.into(),
            error: false,
            sticky: false,
            action: None,
            expire: None,
            leaving: false,
        }
    }
    /// An error, marked in `danger`, that stays until its close button is pressed.
    pub(super) fn error(text: impl Into<SharedString>) -> Self {
        Self {
            error: true,
            sticky: true,
            ..Self::info(text)
        }
    }
    /// One button, e.g. Undo. Pressing it closes the toast without `on_expire`.
    pub(super) fn action(
        mut self,
        label: impl Into<SharedString>,
        action: impl FnOnce(&mut Ocal, &mut Window, &mut Context<Ocal>) + 'static,
    ) -> Self {
        self.action = Some((label.into(), Box::new(action)));
        self
    }
    /// Runs when the toast goes without its action being pressed: it timed out, was
    /// closed, or a newer message replaced it. Deferred work (a delete) commits here.
    pub(super) fn on_expire(
        mut self,
        expire: impl FnOnce(&mut Ocal, &mut Context<Ocal>) + 'static,
    ) -> Self {
        self.expire = Some(Box::new(expire));
        self
    }
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(super) fn sticky(mut self) -> Self {
        self.sticky = true;
        self
    }
}

impl Ocal {
    /// Show a toast and return its id. A new message replaces the current one
    /// (expiring it); errors queue above it until dismissed.
    pub(super) fn show_toast(&mut self, mut toast: Toast, cx: &mut Context<Self>) -> usize {
        self.next_toast += 1;
        let id = self.next_toast;
        toast.id = id;
        let replaced: Vec<usize> = if toast.error {
            let errors: Vec<_> = self
                .toasts
                .iter()
                .filter(|t| t.error)
                .map(|t| t.id)
                .collect();
            errors[..errors.len().saturating_sub(MAX_ERRORS - 1)].to_vec()
        } else {
            self.toasts
                .iter()
                .filter(|t| !t.error)
                .map(|t| t.id)
                .collect()
        };
        for old in replaced {
            self.dismiss_toast(old, cx);
        }
        let sticky = toast.sticky;
        self.toasts.push(toast);
        if !sticky {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(TOAST_LIFE - TOAST_MOTION)
                    .await;
                let _ = this.update(cx, |app, cx| {
                    if let Some(toast) = app.toasts.iter_mut().find(|t| t.id == id) {
                        toast.leaving = true;
                        cx.notify();
                    }
                });
                cx.background_executor().timer(TOAST_MOTION).await;
                let _ = this.update(cx, |app, cx| app.dismiss_toast(id, cx));
            })
            .detach();
        }
        cx.notify();
        id
    }
    /// A message that fades after four seconds.
    pub(super) fn toast(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) -> usize {
        self.show_toast(Toast::info(text), cx)
    }
    /// A message with one action button, e.g. "Deleted “Dentist”" with Undo.
    #[allow(dead_code)]
    pub(super) fn toast_with_action(
        &mut self,
        text: impl Into<SharedString>,
        label: impl Into<SharedString>,
        action: impl FnOnce(&mut Ocal, &mut Window, &mut Context<Ocal>) + 'static,
        cx: &mut Context<Self>,
    ) -> usize {
        self.show_toast(Toast::info(text).action(label, action), cx)
    }
    /// An error that stays until dismissed.
    pub(super) fn toast_error(
        &mut self,
        text: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> usize {
        self.show_toast(Toast::error(text), cx)
    }
    /// Close a toast, running its `on_expire`. Unknown ids are ignored.
    pub(super) fn dismiss_toast(&mut self, id: usize, cx: &mut Context<Self>) {
        if let Some(index) = self.toasts.iter().position(|t| t.id == id) {
            let toast = self.toasts.remove(index);
            if let Some(expire) = toast.expire {
                expire(self, cx);
            }
            cx.notify();
        }
    }
    #[cfg(test)]
    pub(super) fn toast_texts(&self) -> Vec<String> {
        self.toasts.iter().map(|t| t.text.to_string()).collect()
    }
    fn run_toast_action(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.toasts.iter().position(|t| t.id == id) {
            let toast = self.toasts.remove(index);
            if let Some((_, action)) = toast.action {
                action(self, window, cx);
            }
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    /// The toast stack, centred over the bottom of the sheet. `left` is where the
    /// sheet starts.
    pub(super) fn toast_layer(&self, left: gpui_kit::Rems, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let still = cx.reduce_motion();
        // On the inverted bar, theme colours need CSS's `brightness()` nudge to read.
        let on_bar = |color: Hsla| Hsla {
            l: if t.dark() {
                color.l * 0.8
            } else {
                (color.l * 1.6).min(0.8)
            },
            ..color
        };
        let ordered = self
            .toasts
            .iter()
            .filter(|t| t.error)
            .chain(self.toasts.iter().filter(|t| !t.error));
        let pills = ordered.map(|toast| {
            let id = toast.id;
            let action = toast.action.as_ref().map(|(label, _)| {
                button(
                    ("toast-action", id),
                    label.clone(),
                    ButtonVariant::Secondary,
                    cx,
                )
                .debug_selector(|| "toast-action".into())
                .flex_shrink_0()
                .h(rems(1.5))
                .px(rems(0.375))
                .border_0()
                .rounded(rems(0.375))
                .bg(t.bright.opacity(0.))
                .text_size(rems(0.8125))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(on_bar(t.accent))
                .hover(|s| s.bg(t.background.opacity(0.08)))
                .on_click(
                    cx.listener(move |app, _, window, cx| app.run_toast_action(id, window, cx)),
                )
            });
            let close = toast.error.then(|| {
                button(("toast-close", id), "", ButtonVariant::Secondary, cx)
                    .debug_selector(|| "toast-close".into())
                    .accessibility_label("Dismiss")
                    .flex_shrink_0()
                    .size(rems(1.5))
                    .p(rems(0.))
                    .border_0()
                    .rounded(rems(0.375))
                    .bg(t.bright.opacity(0.))
                    .text_color(t.background.opacity(0.6))
                    .hover(|s| s.bg(t.background.opacity(0.08)).text_color(t.background))
                    .child(icon(IconName::X).size(rems(0.875)))
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.dismiss_toast(id, cx);
                        window.focus(&app.focus, cx);
                    }))
            });
            let trailing = action.is_some() || close.is_some();
            let selector = if toast.error { "toast-error" } else { "toast" };
            let pill = div()
                .id(("toast", id))
                .debug_selector(move || selector.into())
                .occlude()
                .relative()
                .max_w(rems(34.))
                .min_w_0()
                .flex()
                .items_center()
                .gap(rems(0.875))
                .py(rems(if trailing { 0.3125 } else { 0.5625 }))
                .pl(rems(0.875))
                .pr(rems(if trailing { 0.375 } else { 0.875 }))
                .rounded(RADIUS_FIELD)
                .bg(t.bright)
                .text_color(t.background)
                .text_size(rems(0.8125))
                .font_weight(FontWeight::MEDIUM)
                .when(toast.error, |p| {
                    p.gap(rems(0.5)).child(
                        icon(IconName::CircleAlert)
                            .size(rems(1.))
                            .flex_shrink_0()
                            .text_color(on_bar(t.danger)),
                    )
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .when(!toast.error, |d| d.whitespace_nowrap().text_ellipsis())
                        .when(toast.error, |d| d.whitespace_normal())
                        .child(toast.text.clone()),
                )
                .children(action)
                .children(close);
            if still {
                pill.into_any_element()
            } else if toast.leaving {
                pill.with_animation(
                    ("toast-out", id),
                    Animation::new(TOAST_MOTION),
                    |pill, delta| pill.opacity(1. - delta).top(px(6. * delta)),
                )
                .into_any_element()
            } else {
                pill.with_animation(
                    ("toast-in", id),
                    Animation::new(TOAST_MOTION).with_easing(gpui_kit::ease_out_quint()),
                    |pill, delta| pill.opacity(delta).top(px(8. * (1. - delta))),
                )
                .into_any_element()
            }
        });
        div()
            .absolute()
            .left(left)
            .right(rems(SHEET_GAP))
            .bottom(rems(SHEET_GAP + 1.5))
            .px(rems(1.))
            .flex()
            .flex_col()
            .items_center()
            .gap(rems(0.5))
            .children(pills)
            .into_any_element()
    }

    /// `?`: a small card of the main keys above the sidebar footer.
    pub(super) fn shortcuts_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let rows: [(&str, &[&'static str]); 9] = [
            ("New event", &["c"]),
            ("Today", &["t"]),
            ("Previous / next", &["[", "]"]),
            ("Day · week · month", &["1", "2", "3"]),
            ("Agenda · tasks", &["4", "5"]),
            ("Move · extend", &["↑", "↓", "shift"]),
            ("Search", &["/"]),
            ("Sidebar", &["b"]),
            ("Close", &["esc"]),
        ];
        div()
            .id("shortcuts-card")
            .debug_selector(|| "shortcuts-card".into())
            .occlude()
            .absolute()
            .left(rems(0.75))
            .bottom(rems(3.25))
            .w(rems(16.25))
            .px(rems(0.875))
            .pt(rems(0.625))
            .pb(rems(0.375))
            .rounded(RADIUS_CARD)
            .bg(t.background)
            .shadow(card_shadow(&t))
            .on_mouse_down_out(cx.listener(|app, _, _, cx| {
                app.keys_open = false;
                cx.notify();
            }))
            .children(rows.into_iter().map(|(label, keys)| {
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .py(rems(0.25))
                    .text_size(rems(0.78))
                    .text_color(t.muted())
                    .child(label)
                    .child(
                        div()
                            .flex()
                            .gap(rems(0.1875))
                            .children(keys.iter().map(|k| soft_keycap(k, false, cx))),
                    )
            }))
            .child(
                div()
                    .mt(rems(0.375))
                    .pt(rems(0.25))
                    .border_t_1()
                    .border_color(t.hair())
                    .child(
                        button("all-shortcuts", "", ButtonVariant::Secondary, cx)
                            .debug_selector(|| "all-shortcuts".into())
                            .w_full()
                            .h(rems(1.75))
                            .px(rems(0.))
                            .justify_between()
                            .border_0()
                            .bg(t.background.opacity(0.))
                            .text_size(rems(0.78))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.accent)
                            .hover(|s| s.text_color(t.bright))
                            .child("All shortcuts")
                            .child(icon(IconName::ChevronRight).size(rems(0.875)))
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.keys_open = false;
                                app.show_help(window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }
}
