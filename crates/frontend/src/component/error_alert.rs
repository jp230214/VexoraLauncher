use gpui::{prelude::*, *};
use gpui_component::{ActiveTheme as _, StyledExt, h_flex, v_flex};

use crate::icon::PandoraIcon;

#[derive(IntoElement)]
pub struct ErrorAlert {
    title: SharedString,
    message: SharedString,
    w: Length,
}

impl ErrorAlert {
    pub fn new(title: SharedString, message: SharedString) -> Self {
        Self {
            title,
            message,
            w: Length::Definite(DefiniteLength::Fraction(1.0)),
        }
    }

    pub fn w(mut self, length: Length) -> Self {
        self.w = length;
        self
    }
}

impl RenderOnce for ErrorAlert {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let radius = theme.radius;
        let padding_x = px(16.0);
        let padding_y = px(12.0);
        let gap = px(12.0);

        let danger = theme.danger;
        let bg = danger.opacity(0.1);
        let fg = theme.red;
        let border_color = danger.opacity(0.6);

        h_flex()
            .w(self.w)
            .text_color(fg)
            .bg(bg)
            .px(padding_x)
            .py(padding_y)
            .gap(gap)
            .justify_between()
            .text_sm()
            .border_1()
            .border_color(border_color)
            .rounded(radius)
            .items_start()
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .gap(gap)
                    .child(
                        div()
                            .mt(px(4.0))
                            .child(PandoraIcon::CircleX)
                            .size_5()
                            .text_color(fg),
                    )
                    .child(
                        v_flex()
                            .overflow_hidden()
                            .gap_1()
                            .child(
                                div()
                                    .w_full()
                                    .text_base()
                                    .font_medium()
                                    .truncate()
                                    .child(self.title),
                            )
                            .child(div().text_color(theme.muted_foreground).child(self.message)),
                    ),
            )
            .into_any_element()
    }
}
