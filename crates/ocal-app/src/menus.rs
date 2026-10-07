//! Small desktop menus with keyboard navigation and a viewport-bounded surface.
use gpui_kit::base::{Popover, Selectable};
use gpui_kit::{
    App, ElementId, Focusable, IntoElement, ScrollHandle, Window, div, point, prelude::*, px, rems,
};
use gpui_omarchy::{ActiveTheme, ButtonVariant, IconName, MenuItem, button, icon, keycaps};
use std::rc::Rc;

pub fn menu(
    id: impl Into<ElementId>,
    trigger: impl Selectable + IntoElement + 'static,
    items: Vec<MenuItem>,
    on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> Popover {
    let on_select = Rc::new(on_select);
    Popover::new(id)
        .trigger(trigger)
        .content(move |state, window, cx| {
            let t = cx.omarchy().clone();
            let page = window.use_keyed_state("menu-page", cx, |_, _| None::<usize>);
            let cursor = window.use_keyed_state("menu-cursor", cx, |_, _| {
                items.iter().position(|item| !item.disabled)
            });
            let scroll = window
                .use_keyed_state("menu-scroll", cx, |_, _| ScrollHandle::new())
                .read(cx)
                .clone();
            let focus = window
                .use_keyed_state("menu-focus", cx, |_, cx| cx.focus_handle())
                .read(cx)
                .clone();
            if state.focus_handle(cx).is_focused(window) {
                focus.focus(window, cx);
            }
            let open_page = *page.read(cx);
            let entries: Vec<_> = match open_page {
                Some(index) => items[index].children.clone(),
                None => items.iter().cloned().enumerate().collect(),
            };
            let entries = Rc::new(entries);
            let select_page = page.clone();
            let select_cursor = cursor.clone();
            let select_scroll = scroll.clone();
            let displayed = entries.clone();
            let dismiss = cx.entity();
            let select = Rc::new(move |index: usize, window: &mut Window, cx: &mut App| {
                let Some((action, item)) = displayed.get(index) else {
                    return;
                };
                if item.disabled {
                    return;
                }
                if !item.children.is_empty() {
                    select_page.update(cx, |page, cx| {
                        *page = Some(*action);
                        cx.notify();
                    });
                    select_cursor.update(cx, |cursor, cx| {
                        *cursor = item.children.iter().position(|(_, item)| !item.disabled);
                        cx.notify();
                    });
                    select_scroll.set_offset(point(px(0.), px(0.)));
                } else {
                    dismiss.update(cx, |state, cx| state.dismiss(window, cx));
                    on_select(*action, window, cx);
                }
                window.refresh();
            });
            let confirm = select.clone();
            let confirm_cursor = cursor.clone();
            let key_cursor = cursor.clone();
            let key_page = page.clone();
            let key_scroll = scroll.clone();
            let key_entries = entries.clone();
            let key_select = select.clone();
            let highlighted = *cursor.read(cx);
            let mut surface = div()
                .id("menu-surface")
                .debug_selector(|| "omarchy-menu-content".into())
                .role(gpui_kit::Role::Menu)
                .track_focus(&focus)
                .key_context("CalendarMenu")
                .w(rems(16.))
                .max_w(window.viewport_size().width - px(12.))
                .max_h(window.viewport_size().height - px(12.))
                .border_1()
                .border_color(t.control_border())
                .bg(t.background)
                .text_color(t.foreground)
                .text_size(rems(0.75))
                .font_family(t.font.clone())
                .on_action(move |_: &gpui_kit::base::actions::Confirm, window, cx| {
                    cx.stop_propagation();
                    if let Some(index) = *confirm_cursor.read(cx) {
                        confirm(index, window, cx);
                    }
                })
                .on_key_down(move |event, window, cx| {
                    if event.keystroke.modifiers.modified() {
                        return;
                    }
                    match event.keystroke.key.as_str() {
                        "up" | "k" | "down" | "j" | "home" | "end" => {
                            let enabled: Vec<_> = key_entries
                                .iter()
                                .enumerate()
                                .filter_map(|(i, (_, item))| (!item.disabled).then_some(i))
                                .collect();
                            if enabled.is_empty() {
                                return;
                            }
                            let current = key_cursor
                                .read(cx)
                                .and_then(|index| enabled.iter().position(|&i| i == index));
                            let next = match event.keystroke.key.as_str() {
                                "home" => 0,
                                "end" => enabled.len() - 1,
                                "up" | "k" => current.map_or(enabled.len() - 1, |i| {
                                    (i + enabled.len() - 1) % enabled.len()
                                }),
                                _ => current.map_or(0, |i| (i + 1) % enabled.len()),
                            };
                            let next = enabled[next];
                            key_cursor.update(cx, |cursor, cx| {
                                *cursor = Some(next);
                                cx.notify();
                            });
                            key_scroll.scroll_to_item(next);
                            window.refresh();
                        }
                        "right" => {
                            if let Some(index) = *key_cursor.read(cx)
                                && !key_entries[index].1.children.is_empty()
                            {
                                key_select(index, window, cx);
                            }
                        }
                        "left" if open_page.is_some() => {
                            key_page.update(cx, |page, cx| {
                                *page = None;
                                cx.notify();
                            });
                            key_cursor.update(cx, |cursor, cx| {
                                *cursor = open_page;
                                cx.notify();
                            });
                            key_scroll.scroll_to_item(open_page.unwrap());
                            window.refresh();
                        }
                        _ => return,
                    }
                    cx.stop_propagation();
                });
            if let Some(parent) = open_page {
                let back_page = page.clone();
                let back_cursor = cursor.clone();
                let back_scroll = scroll.clone();
                surface = surface.child(
                    button("menu-back", "", ButtonVariant::Secondary, cx)
                        .w_full()
                        .border_0()
                        .justify_start()
                        .px(rems(0.75))
                        .py(rems(0.5))
                        .child(icon(IconName::ChevronLeft).size(rems(0.85)))
                        .child(items[parent].label.clone())
                        .on_click(move |_, window, cx| {
                            back_page.update(cx, |page, cx| {
                                *page = None;
                                cx.notify();
                            });
                            back_cursor.update(cx, |cursor, cx| {
                                *cursor = Some(parent);
                                cx.notify();
                            });
                            back_scroll.scroll_to_item(parent);
                            window.refresh();
                        }),
                );
            }
            surface.child(
                div()
                    .id("menu-scroll")
                    .debug_selector(|| "calendar-menu-scroll".into())
                    .overflow_y_scroll()
                    .max_h(
                        window.viewport_size().height
                            - rems(if open_page.is_some() { 3.2 } else { 0.7 })
                                .to_pixels(window.rem_size()),
                    )
                    .track_scroll(&scroll)
                    .p(rems(0.3))
                    .children(entries.iter().enumerate().map(|(index, (_, item))| {
                        let choose = select.clone();
                        let hover = cursor.clone();
                        let disabled = item.disabled;
                        let danger = item.icon == Some(IconName::Trash);
                        let ink = if danger { t.danger } else { t.foreground };
                        div()
                            .when(item.separator_before, |row| {
                                row.child(div().h(px(1.)).my(rems(0.25)).bg(t.divider()))
                            })
                            .child(
                                button(("menu-item", index), "", ButtonVariant::Secondary, cx)
                                    .debug_selector(move || format!("calendar-menu-item-{index}"))
                                    .accessibility_label(item.label.clone())
                                    .role(if item.checked.is_some() {
                                        gpui_kit::Role::MenuItemCheckBox
                                    } else {
                                        gpui_kit::Role::MenuItem
                                    })
                                    .when_some(item.checked, |row, checked| {
                                        row.aria_toggled(if checked {
                                            gpui_kit::accesskit::Toggled::True
                                        } else {
                                            gpui_kit::accesskit::Toggled::False
                                        })
                                    })
                                    .focusable(false)
                                    .disabled(disabled)
                                    .w_full()
                                    .h(rems(1.9))
                                    .border_0()
                                    .px(rems(0.5))
                                    .py(rems(0.))
                                    .gap(rems(0.55))
                                    .justify_start()
                                    .text_color(ink)
                                    .bg(if highlighted == Some(index) {
                                        t.hover_fill()
                                    } else {
                                        t.foreground.opacity(0.)
                                    })
                                    .hover(|s| s.bg(t.hover_fill()).text_color(ink))
                                    .child(
                                        div()
                                            .w(rems(0.95))
                                            .flex_shrink_0()
                                            .when_some(item.icon, |slot, name| {
                                                slot.child(icon(name).size(rems(0.9)))
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_ellipsis()
                                            .child(item.label.clone()),
                                    )
                                    .when_some(item.shortcut.as_ref(), |row, hint| {
                                        row.child(keycaps(
                                            hint.split_whitespace().map(str::to_owned),
                                            cx,
                                        ))
                                    })
                                    .when_some(item.checked, |row, checked| {
                                        row.child(div().w(rems(0.85)).when(checked, |mark| {
                                            mark.child(icon(IconName::Check).size(rems(0.8)))
                                        }))
                                    })
                                    .when(!item.children.is_empty(), |row| {
                                        row.child(icon(IconName::ChevronRight).size(rems(0.85)))
                                    })
                                    .on_mouse_move(move |_, window, cx| {
                                        if !disabled {
                                            hover.update(cx, |cursor, cx| {
                                                *cursor = Some(index);
                                                cx.notify();
                                            });
                                            window.refresh();
                                        }
                                    })
                                    .on_click(move |_, window, cx| choose(index, window, cx)),
                            )
                    })),
            )
        })
}

/// A context menu target that keeps its own selection and focus semantics.
pub struct ContextTarget<E>(pub E);
impl<E> Selectable for ContextTarget<E> {
    fn is_selected(&self) -> bool {
        false
    }
    fn selected(self, _: bool) -> Self {
        self
    }
}
impl<E: IntoElement> IntoElement for ContextTarget<E> {
    type Element = E::Element;
    fn into_element(self) -> Self::Element {
        self.0.into_element()
    }
}
