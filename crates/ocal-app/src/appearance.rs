use gpui_kit::{App, ElementId, prelude::*, rems};
use gpui_omarchy::{ActiveTheme, Button, ButtonVariant, IconName, button, icon, with_tooltip};

pub fn tool_button(
    id: impl Into<ElementId>,
    glyph: IconName,
    label: &'static str,
    shortcut: &'static str,
    cx: &App,
) -> Button {
    let t = cx.omarchy();
    with_tooltip(
        button(id, "", ButtonVariant::Secondary, cx)
            .debug_selector(move || format!("tool-{label}"))
            .accessibility_label(label)
            .size(rems(1.9))
            .p(rems(0.))
            .border_0()
            .bg(t.foreground.opacity(0.))
            .text_color(t.secondary)
            .hover(|s| s.bg(t.hover_fill()).text_color(t.bright))
            .active(|s| s.bg(t.selection).text_color(t.accent))
            .focus_visible(|s| s.border_1().border_color(t.accent).text_color(t.accent))
            .child(icon(glyph).size(rems(1.))),
        if shortcut.is_empty() {
            label.to_owned()
        } else {
            format!("{label} · {shortcut}")
        },
    )
}

pub fn action_button(
    id: impl Into<ElementId>,
    glyph: IconName,
    label: &'static str,
    variant: ButtonVariant,
    cx: &App,
) -> Button {
    button(id, "", variant, cx)
        .accessibility_label(label)
        .text_size(rems(0.78))
        .gap(rems(0.45))
        .child(icon(glyph).size(rems(0.95)))
        .child(label)
}

/// Reveal a newly focused composer field without disturbing manual scrolling.
pub fn reveal_field(
    element: impl gpui_kit::IntoElement + 'static,
    reveal: bool,
    scroll: &gpui_kit::ScrollHandle,
) -> gpui_kit::AnyElement {
    use gpui_kit::base::ElementExt;
    let scroll = scroll.clone();
    gpui_kit::div()
        .child(element)
        .on_prepaint(move |bounds, window, _| {
            if !reveal {
                return;
            }
            let viewport = scroll.bounds();
            if viewport.size.height <= gpui_kit::px(0.) {
                return;
            }
            let delta = if bounds.top() < viewport.top()
                || (bounds.size.height > viewport.size.height && bounds.top() > viewport.top())
            {
                viewport.top() - bounds.top()
            } else if bounds.bottom() > viewport.bottom() {
                viewport.bottom() - bounds.bottom()
            } else {
                return;
            };
            let next = gpui_kit::point(
                scroll.offset().x,
                (scroll.offset().y + delta).clamp(-scroll.max_offset().y, gpui_kit::px(0.)),
            );
            scroll.set_offset(next);
            window.refresh();
        })
        .into_any_element()
}
