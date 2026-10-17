use gpui_kit::{
    App, AppContext, BoxShadow, Context, ElementId, FontWeight, Hsla, Rems, Render, SharedString,
    Stateful, StatefulInteractiveElement, Window, div, point, prelude::*, px, rems,
};
use gpui_omarchy::{ActiveTheme, Button, ButtonVariant, IconName, Theme, button, icon};

/// Small fixed corners: event blocks, all-day pills, quiet buttons, tooltips.
pub const RADIUS_SMALL: Rems = rems(0.4375);
/// Icon buttons and primary buttons.
pub const RADIUS_CONTROL: Rems = rems(0.5);
/// The search field and the sidebar's New event button.
pub const RADIUS_FIELD: Rems = rems(0.625);
/// Floating cards, popovers and the Now card.
pub const RADIUS_CARD: Rems = rems(0.75);
/// The raised content sheet.
pub const RADIUS_SHEET: Rems = rems(0.75);

/// Text and line greys derived from the live theme, as in Omail: the palette has
/// no tokens for them, so they are `foreground` at fixed opacities.
pub trait Tones {
    /// Secondary text.
    fn muted(&self) -> Hsla;
    /// Tertiary text: weekday initials, hour labels, keycaps.
    fn faint(&self) -> Hsla;
    /// Hairline dividers.
    fn hair(&self) -> Hsla;
    /// Hover and quiet control fills.
    fn hover(&self) -> Hsla;
    /// Weekend and off-hours wash: darker on dark themes, a faint ink on light ones.
    fn shade(&self) -> Hsla;
    fn dark(&self) -> bool;
}
impl Tones for Theme {
    fn muted(&self) -> Hsla {
        self.foreground.opacity(0.58)
    }
    fn faint(&self) -> Hsla {
        self.foreground.opacity(0.38)
    }
    fn hair(&self) -> Hsla {
        self.foreground.opacity(0.07)
    }
    fn hover(&self) -> Hsla {
        self.foreground.opacity(0.05)
    }
    fn shade(&self) -> Hsla {
        if self.dark() {
            gpui_kit::black().opacity(0.13)
        } else {
            self.foreground.opacity(0.025)
        }
    }
    fn dark(&self) -> bool {
        self.background.l < 0.5
    }
}

/// Light or dark chosen in Ocal, or whatever the Omarchy theme is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}
impl Appearance {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.name() == name)
    }
    /// The stored preference value.
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
    /// Apply the palette. An override keeps the system palette when it already
    /// has the chosen brightness, and otherwise uses Omail's pair.
    pub fn apply(self, cx: &mut App) {
        let system = || Theme::system_or_default();
        match self {
            Self::System => Theme::follow_system(cx),
            Self::Light => Some(system())
                .filter(|t| !t.dark())
                .unwrap_or_else(Theme::flexoki_light)
                .apply(cx),
            Self::Dark => Some(system())
                .filter(|t| t.dark())
                .unwrap_or_else(Theme::tokyo_night)
                .apply(cx),
        }
    }
}

/// `amount` of `color` mixed into `towards` in sRGB, like CSS `color-mix()`.
/// A translucent `towards` is first laid over the theme background, so the result
/// is always opaque.
pub fn mix(t: &Theme, color: Hsla, amount: f32, towards: Hsla) -> Hsla {
    let (a, b) = (color.to_rgb(), t.background.blend(towards).to_rgb());
    let channel = |x: f32, y: f32| x * amount + y * (1. - amount);
    gpui_kit::Rgba {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
        a: 1.,
    }
    .into()
}

/// The raised content sheet: a faint ring and a soft drop.
pub fn sheet_shadow(t: &Theme) -> Vec<BoxShadow> {
    let (ring, drop, y, blur) = if t.dark() {
        (
            t.foreground.opacity(0.06),
            gpui_kit::black().opacity(0.35),
            12.,
            40.,
        )
    } else {
        (
            t.foreground.opacity(0.07),
            t.foreground.opacity(0.08),
            10.,
            30.,
        )
    };
    vec![shadow(ring, 0., 0., 1.), shadow(drop, y, blur, 0.)]
}

/// Popovers, composer and other floating cards.
pub fn card_shadow(t: &Theme) -> Vec<BoxShadow> {
    let (drop, y, blur) = if t.dark() {
        (gpui_kit::black().opacity(0.34), 12., 34.)
    } else {
        (t.foreground.opacity(0.12), 10., 28.)
    };
    vec![shadow(t.hair(), 0., 0., 1.), shadow(drop, y, blur, 0.)]
}

/// A soft accent-coloured glow under a primary button.
pub fn accent_glow(t: &Theme) -> Vec<BoxShadow> {
    vec![shadow(t.accent.opacity(0.28), 6., 18., 0.)]
}

/// An outer shadow; `spread` without blur draws a ring.
pub fn shadow(color: Hsla, y: f32, blur: f32, spread: f32) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset: false,
    }
}

/// A ring drawn inside the element's bounds, e.g. hollow checkboxes and focus rings.
pub fn inset_ring(color: Hsla, width: f32) -> Vec<BoxShadow> {
    vec![BoxShadow {
        inset: true,
        ..shadow(color, 0., 0., width)
    }]
}

/// Tooltip text in the "Label · key" form, with lower-case keys.
pub fn tip_text(label: &str, shortcut: &str) -> String {
    if shortcut.is_empty() {
        label.to_owned()
    } else {
        format!("{label} · {}", shortcut.to_lowercase())
    }
}

/// Attach an inverted, bright tooltip label, as in Omail's prototype. It is
/// gpui-omarchy's tooltip surface and timing, restyled.
pub fn with_tip<T: StatefulInteractiveElement>(control: T, text: impl Into<SharedString>) -> T {
    let text = text.into();
    control
        .tooltip_show_delay(std::time::Duration::from_millis(400))
        .tooltip(move |_, cx| cx.new(|_| Tip(text.clone())).into())
}
struct Tip(SharedString);
impl Render for Tip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.omarchy();
        div().debug_selector(|| "tooltip".into()).child(
            gpui_omarchy::tooltip::tooltip(self.0.clone(), cx)
                .border_0()
                .rounded(RADIUS_SMALL)
                .px(rems(0.5))
                .py(rems(0.3125))
                .bg(t.bright)
                .text_color(t.background)
                .text_size(rems(0.72))
                .font_weight(FontWeight::MEDIUM),
        )
    }
}

/// A 30 px muted icon button with a hover fill.
pub fn tool_button(
    id: impl Into<ElementId>,
    glyph: IconName,
    label: &'static str,
    shortcut: &'static str,
    cx: &App,
) -> Button {
    let t = cx.omarchy();
    with_tip(
        button(id, "", ButtonVariant::Secondary, cx)
            .debug_selector(move || format!("tool-{label}"))
            .accessibility_label(label)
            .size(rems(1.875))
            .flex_shrink_0()
            .p(rems(0.))
            .rounded(RADIUS_CONTROL)
            .border_0()
            .bg(t.foreground.opacity(0.))
            .text_color(t.muted())
            .hover(|s| s.bg(t.hover()).text_color(t.bright))
            .active(|s| s.bg(t.selection).text_color(t.accent))
            .focus_visible(|s| s.shadow(inset_ring(t.accent, 1.5)).text_color(t.accent))
            .child(icon(glyph).size(rems(1.))),
        tip_text(label, shortcut),
    )
}

/// A fully rounded tab: view switches, calendar sets and filters.
pub fn pill_tab(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
    cx: &App,
) -> Button {
    let t = cx.omarchy();
    button(id, label, ButtonVariant::Secondary, cx)
        .h(rems(1.625))
        .px(rems(0.625))
        .gap(rems(0.45))
        .flex_shrink_0()
        .border_0()
        .rounded_full()
        .text_size(rems(0.78))
        .bg(if active {
            t.selection
        } else {
            t.foreground.opacity(0.)
        })
        .text_color(if active { t.bright } else { t.muted() })
        .when(active, |b| b.font_weight(FontWeight::MEDIUM))
        .hover(|s| s.text_color(t.bright))
        .focus_visible(|s| s.shadow(inset_ring(t.accent, 1.5)))
}

/// A small keycap for quiet surfaces, or for an accent button when `on_accent`.
pub fn soft_keycap(key: &'static str, on_accent: bool, cx: &App) -> gpui_kit::Div {
    let t = cx.omarchy();
    div()
        .flex_shrink_0()
        .px(rems(0.3125))
        .py(rems(0.1875))
        .rounded(rems(0.25))
        .bg(if on_accent {
            gpui_kit::black().opacity(0.13)
        } else {
            t.hover()
        })
        .text_color(if on_accent { t.on_accent } else { t.faint() })
        .font_family(t.mono_font.clone())
        .text_size(rems(0.66))
        .font_weight(FontWeight::MEDIUM)
        .line_height(gpui_kit::relative(1.))
        .child(key)
}

/// The primary action: accent fill, optional leading icon and trailing keycap.
pub fn send_button(
    id: impl Into<ElementId>,
    label: &'static str,
    glyph: Option<IconName>,
    key: Option<&'static str>,
    cx: &App,
) -> Button {
    let t = cx.omarchy();
    let hover = Hsla {
        l: (t.accent.l * 1.08).min(1.),
        ..t.accent
    };
    button(id, "", ButtonVariant::Primary, cx)
        .accessibility_label(label)
        .h(rems(1.875))
        .px(rems(0.75))
        .gap(rems(0.45))
        .flex_shrink_0()
        .rounded(RADIUS_CONTROL)
        .border_0()
        .bg(t.accent)
        .text_color(t.on_accent)
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(rems(0.78))
        .hover(move |s| s.bg(hover))
        .active(move |s| s.bg(hover))
        .focus_visible(move |s| s.bg(hover).shadow(inset_ring(t.on_accent, 1.5)))
        .justify_start()
        .children(glyph.map(|g| icon(g).size(rems(1.))))
        // The label takes any spare width so a wide button keeps its keycap at the end.
        .child(div().flex_grow(1.).child(label))
        .children(key.map(|k| soft_keycap(k, true, cx)))
}

/// A quiet text button with an optional icon, e.g. Edit and Duplicate in a card.
pub fn mode_button(
    id: impl Into<ElementId>,
    label: &'static str,
    glyph: Option<IconName>,
    cx: &App,
) -> Button {
    let t = cx.omarchy();
    button(id, "", ButtonVariant::Secondary, cx)
        .accessibility_label(label)
        .h(rems(1.75))
        .px(rems(0.5625))
        .gap(rems(0.375))
        .flex_shrink_0()
        .rounded(RADIUS_SMALL)
        .border_0()
        .bg(t.foreground.opacity(0.))
        .text_color(t.muted())
        .text_size(rems(0.78))
        .hover(|s| s.bg(t.hover()).text_color(t.bright))
        .active(|s| s.bg(t.selection).text_color(t.bright))
        .focus_visible(|s| s.shadow(inset_ring(t.accent, 1.5)).text_color(t.bright))
        .children(glyph.map(|g| icon(g).size(rems(0.875))))
        .child(label)
}

/// A calendar or label name in its colour on a soft tint of it.
pub fn label_chip(name: impl Into<SharedString>, color: Hsla) -> gpui_kit::Div {
    div()
        .flex_shrink_0()
        .px(rems(0.4375))
        .py(rems(0.0625))
        .rounded_full()
        .bg(color.opacity(0.15))
        .text_color(color)
        .text_size(rems(0.69))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .child(name.into())
}

/// A round task checkbox in the calendar's colour: a hollow ring, or filled with a
/// tick when done. Clicks on it should stop propagation so rows don't also open.
pub fn task_check(
    id: impl Into<ElementId>,
    color: Hsla,
    done: bool,
    size: Rems,
    cx: &App,
) -> Stateful<gpui_kit::Div> {
    let t = cx.omarchy();
    div()
        .id(id)
        .size(size)
        .flex_shrink_0()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .map(|d| {
            if done {
                d.bg(color).child(
                    icon(IconName::Check)
                        .size(rems(size.0 * 0.7))
                        .text_color(t.on_accent),
                )
            } else {
                d.shadow(inset_ring(color, 1.5))
                    .hover(move |s| s.bg(color.opacity(0.15)))
            }
        })
}

/// A short vertical hairline between toolbar groups.
pub fn separator(cx: &App) -> gpui_kit::Div {
    div()
        .w(px(1.))
        .h(rems(1.))
        .mx(rems(0.375))
        .flex_shrink_0()
        .bg(cx.omarchy().hair())
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
pub fn tint(t: &Theme, color: Hsla, amount: f32) -> Hsla {
    t.background.blend(color.opacity(amount))
}
/// Calendar colours are often pastel and fail contrast as text on their own tint,
/// so event text is the colour mixed halfway towards `bright`.
pub fn event_text(t: &Theme, color: Hsla) -> Hsla {
    mix(t, color, 0.5, t.bright)
}
/// A round day number, filled with the accent colour when it is today.
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
        .px(rems(0.25))
        .rounded_full()
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
