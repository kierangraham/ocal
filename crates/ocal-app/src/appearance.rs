use gpui_kit::{App, ElementId, FontWeight, div, prelude::*, rems};
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

/// An opaque calendar-coloured fill over the theme background.
pub fn tint(t: &gpui_omarchy::Theme, color: gpui_kit::Hsla, amount: f32) -> gpui_kit::Hsla {
    t.background.blend(color.opacity(amount))
}
/// Calendar colours are often pastel; on light themes they fail contrast as text.
pub fn event_text(t: &gpui_omarchy::Theme, color: gpui_kit::Hsla) -> gpui_kit::Hsla {
    if t.background.l > 0.5 {
        t.bright
    } else {
        color
    }
}
/// A day number, filled with the accent colour when it is today.
pub fn day_number(
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
        .text_size(size)
        .when(today, |d| {
            d.bg(t.accent)
                .text_color(t.on_accent)
                .font_weight(FontWeight::SEMIBOLD)
        })
        .when(!today, |d| d.text_color(color))
        .child(day.to_string())
}

/// An uppercase sidebar or page section label.
pub fn section_label(text: &'static str, cx: &App) -> gpui_kit::Div {
    div()
        .px(rems(0.75))
        .pt(rems(1.))
        .pb(rems(0.35))
        .text_size(rems(0.62))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.omarchy().secondary)
        .child(text)
}

/// A full-width list row; the active row gets the selection fill and an accent bar.
pub fn nav_row(id: impl Into<ElementId>, active: bool, cx: &App) -> Button {
    let t = cx.omarchy();
    button(id, "", ButtonVariant::Secondary, cx)
        .relative()
        .w_full()
        .h(rems(1.9))
        .px(rems(0.75))
        .gap(rems(0.6))
        .justify_start()
        .border_0()
        .text_size(rems(0.78))
        .bg(if active {
            t.selection
        } else {
            t.foreground.opacity(0.)
        })
        .hover(|s| s.bg(t.hover_fill()))
        .children(active.then(|| {
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(gpui_kit::px(2.))
                .bg(t.accent)
        }))
}

/// A square calendar colour swatch, outlined when the calendar is hidden.
pub fn swatch(color: gpui_kit::Hsla, filled: bool) -> gpui_kit::Div {
    div()
        .size(rems(0.7))
        .flex_shrink_0()
        .border_1()
        .border_color(color)
        .when(filled, |d| d.bg(color))
}

/// A centred icon, title and explanation for an empty view.
pub fn empty_state(
    glyph: IconName,
    title: &'static str,
    detail: &'static str,
    cx: &App,
) -> gpui_kit::Div {
    let t = cx.omarchy();
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(rems(0.5))
        .p(rems(2.))
        .text_center()
        .child(
            icon(glyph)
                .size(rems(1.8))
                .text_color(t.secondary.opacity(0.7)),
        )
        .child(div().text_color(t.bright).child(title))
        .child(
            div()
                .max_w(rems(18.))
                .text_size(rems(0.73))
                .text_color(t.secondary)
                .whitespace_normal()
                .child(detail),
        )
}
