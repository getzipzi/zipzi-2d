use super::super::{fixture, harness::Harness};
use super::common::save;
use editor_core::{Editor, ObjectKind, ObjectRef};
use gpui_kit::{point, px, test::TestWindowExt};

pub(crate) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    for locale in ["en", "zh-CN"] {
        let mut harness = Harness::new()?;
        crate::i18n::activate(Some(locale));
        let (mut document, _assets) = fixture::sample_document_with_parts()?;
        document.navigate(fixture::object(ObjectKind::Part, 101))?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.editor = Editor::from_document(document);
                view.refresh_document(window, cx);
            });
            window.render_frame(cx);
            window.click("property-group-$details", cx);
        })?;
        save(
            &harness.capture()?,
            directory,
            &format!("properties-part-{locale}.png"),
        )?;
        let (document, _assets) = fixture::sample_document_with_parameter_groups()?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.editor = Editor::from_document(document);
                view.refresh_document(window, cx);
            });
            window.render_frame(cx);
        })?;
        save(
            &harness.capture()?,
            directory,
            &format!("properties-details-{locale}.png"),
        )?;

        let (mut document, _assets) = fixture::sample_document_with_parameters(1)?;
        document.navigate(ObjectRef::new(
            ObjectKind::Parameter,
            &document.parameters()[0].id,
        ))?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.editor = Editor::from_document(document);
                view.refresh_document(window, cx);
            });
        })?;
        harness.capture()?;
        for (field, value) in [
            ("maximum", "20"),
            ("default_value", "15"),
            ("minimum", "10"),
            ("minimum", "25"),
        ] {
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.activate_window();
                for _ in 0..3 {
                    window.scroll(
                        "object-properties",
                        gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-100_000.))),
                        cx,
                    );
                    window.render_frame(cx);
                }
                window.click(format!("property-input-{field}"), cx);
                window.press("cmd-a", cx);
                window.input(value, cx);
                window.press("enter", cx);
            })?;
            harness.capture()?;
        }
        save(
            &harness.capture()?,
            directory,
            &format!("properties-range-{locale}.png"),
        )?;
    }
    Ok(())
}
