//! Application identity, project links, and license notices.
use crate::t;
use gpui_kit::base::Link;
use gpui_kit::component::{ActiveTheme, WindowExt};
use gpui_kit::{prelude::*, *};

const REPOSITORY: &str = "https://github.com/pakholeung37/kasane-editor";

pub(crate) fn show(window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, |dialog, _, cx| {
        dialog.title(t!("app.about")).width(px(480.)).child(
            div()
                .flex()
                .flex_col()
                .gap_4()
                .text_size(super::compact::rem(super::compact::BODY))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Kasane Editor"),
                        )
                        .child(
                            div()
                                .text_color(cx.theme().muted_foreground)
                                .child(t!("about.version", version = env!("CARGO_PKG_VERSION"))),
                        )
                        .child(t!("app.description")),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap_2()
                        .child(link("about-repository", REPOSITORY, REPOSITORY, cx))
                        .child(link(
                            "about-issues",
                            t!("about.report_issue"),
                            format!("{REPOSITORY}/issues"),
                            cx,
                        ))
                        .child(link(
                            "about-license",
                            t!("about.license", license = env!("CARGO_PKG_LICENSE")),
                            format!("{REPOSITORY}/blob/HEAD/LICENSE"),
                            cx,
                        )),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .pt_3()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("about.copyright"))
                        .child(t!("about.credits"))
                        .child(t!("about.third_party"))
                        .child(t!("about.warranty")),
                ),
        )
    });
}

fn link(
    id: &'static str,
    label: impl Into<SharedString>,
    href: impl Into<SharedString>,
    cx: &App,
) -> impl IntoElement {
    Link::new(id)
        .href(href)
        .open_with(|href, _, _, cx| cx.open_url(href))
        .text_color(cx.theme().link)
        .underline()
        .child(label.into())
}

#[cfg(test)]
mod tests {
    use super::{REPOSITORY, link};
    use gpui_kit::{Context, IntoElement, Modifiers, Render, TestAppContext, Window, point, px};

    struct RepositoryLink;

    impl Render for RepositoryLink {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            link("about-repository", REPOSITORY, REPOSITORY, cx)
        }
    }

    #[gpui_kit::test]
    fn github_click_opens_external_url(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (_, cx) = cx.add_window_view(|_, _| RepositoryLink);
        cx.update(|window, cx| window.draw(cx).clear(cx));

        cx.simulate_click(point(px(10.), px(10.)), Modifiers::default());

        assert_eq!(cx.opened_url().as_deref(), Some(REPOSITORY));
    }
}
