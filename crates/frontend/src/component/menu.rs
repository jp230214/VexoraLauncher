use std::rc::Rc;

use gpui::{
    App, ClickEvent, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder,
};
use gpui_component::{ActiveTheme, Icon, StyledExt, h_flex, v_flex};

use crate::icon::PandoraIcon;

#[derive(IntoElement)]
pub struct MenuGroup {
    title: SharedString,
    children: Vec<MenuGroupItem>,
}

impl MenuGroup {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            children: Vec::new(),
        }
    }

    pub fn child(mut self, child: MenuGroupItem) -> Self {
        self.children.push(child);
        self
    }
}

impl RenderOnce for MenuGroup {
    fn render(self, _window: &mut gpui::Window, cx: &mut gpui::App) -> impl gpui::IntoElement {
        let theme = cx.theme();
        // Section header: uppercase micro-label with a trailing hairline.
        let title = h_flex()
            .px_2()
            .gap_2()
            .items_center()
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(theme.sidebar_foreground.opacity(0.55))
                    .child(self.title),
            )
            .child(
                div()
                    .h(px(1.0))
                    .flex_1()
                    .bg(theme.sidebar_border.opacity(0.6)),
            );

        v_flex()
            .gap_1()
            .overflow_x_hidden()
            .child(title)
            .children(self.children)
    }
}

#[derive(IntoElement)]
pub struct MenuGroupItem {
    title: SharedString,
    icon: Option<PandoraIcon>,
    badge: Option<SharedString>,
    active: bool,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
}

impl MenuGroupItem {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            icon: None,
            badge: None,
            active: false,
            on_click: None,
        }
    }

    pub fn icon(mut self, icon: PandoraIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn badge(mut self, badge: impl Into<SharedString>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for MenuGroupItem {
    fn render(self, _window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let theme = cx.theme();
        // Row: [accent bar] [icon] label [badge]. Active rows get a filled
        // pill + left indicator bar; inactive rows are muted until hover.
        let mut item = h_flex()
            .id(self.title.clone())
            .relative()
            .pl_2()
            .pr_2()
            .py_1()
            .gap_2()
            .w_full()
            .items_center()
            .overflow_x_hidden()
            .whitespace_nowrap()
            .rounded(theme.radius)
            .text_sm()
            .when_some(self.icon, |this, icon| {
                this.child(
                    Icon::new(icon)
                        .size_4()
                        .min_w_4()
                        .text_color(theme.sidebar_foreground.opacity(0.8)),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(self.title),
            )
            .when_some(self.badge, |this, badge| {
                this.child(
                    div()
                        .px_1()
                        .py_px()
                        .rounded_full()
                        .text_xs()
                        .bg(theme.sidebar_accent)
                        .text_color(theme.sidebar_accent_foreground)
                        .child(badge),
                )
            })
            .when_some(self.on_click, |this, on_click| {
                this.on_click(move |event, window, cx| {
                    (on_click)(event, window, cx);
                })
            });

        if self.active {
            item = item
                .font_medium()
                .bg(theme.sidebar_accent)
                .text_color(theme.sidebar_accent_foreground)
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_1()
                        .bottom_1()
                        .w(px(3.0))
                        .rounded_full()
                        .bg(theme.sidebar_primary),
                );
        } else {
            item = item
                .text_color(theme.sidebar_foreground.opacity(0.85))
                .hover(|this| {
                    this.bg(theme.sidebar_accent.opacity(0.6))
                        .text_color(theme.sidebar_accent_foreground)
                })
        }

        item
    }
}

fn px(v: f32) -> gpui::Pixels {
    gpui::px(v)
}
