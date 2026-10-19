use super::super::{fixture, harness::Harness};
use super::common::save;
use editor_core::{Editor, ObjectKind, ObjectRef};
use gpui_kit::{point, px, test::TestWindowExt};

pub(crate) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    for locale in ["en", "zh-CN"] {
        let mut harness = Harness::new()?;
        crate::i18n::activate(Some(locale));
        let (document, _assets) = fixture::sample_document()?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.editor = Editor::from_document(document);
                view.refresh_document(window, cx);
            });
            window.render_frame(cx);
            show(window, "property-custom-draw_order", cx);
            window.click("property-custom-draw_order", cx);
            window.render_frame(cx);
        })?;
        for (field, value) in [("draw_order", "42"), ("opacity", "60")] {
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.activate_window();
                show(window, &format!("property-input-{field}"), cx);
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
            &format!("properties-mesh-{locale}.png"),
        )?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            show(window, "property-input-multiply", cx);
            window.click("property-input-multiply", cx);
            window.press("cmd-a", cx);
            window.input("#FFB080", cx);
            window.press("enter", cx);
        })?;
        harness.capture()?;
        harness.view.read_with(&harness.cx, |view, _| {
            let doc = view.editor.document().unwrap();
            let mesh = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
            assert_eq!(
                doc.property_values(&mesh).unwrap()[&editor_core::property::mesh::MULTIPLY.key],
                editor_core::PropertyValue::Color([1., 176. / 255., 128. / 255.]),
            );
        });
        harness.cx.update_window(harness.window, |_, window, cx| {
            show(window, "property-color-screen", cx);
            window.click("property-color-screen", cx);
            window.render_frame(cx);
        })?;
        save(
            &harness.capture()?,
            directory,
            &format!("properties-mesh-color-{locale}.png"),
        )?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            window.click("color-picker-hex", cx);
            window.press("cmd-a", cx);
            window.input("#4080D4", cx);
            window.render_frame(cx);
            window.click("HSV", cx);
            window.render_frame(cx);
            let panel = window.find("color-picker-panel").bounds();
            let wheel = window.find("color-picker-hue").bounds();
            let top = wheel.top() - panel.top();
            assert!((wheel.left() - panel.left() - top).abs() <= px(1.));
            assert!((panel.right() - wheel.right() - top).abs() <= px(1.));
            let input_height = window.find("color-picker-channel-0").bounds().size.height;
            for id in ["RGB", "HSV", "color-picker-cancel", "color-picker-done"] {
                let button = window.find(id).bounds();
                assert!(
                    panel.contains(&button.center()) && button.right() <= panel.right(),
                    "{id} overflows the narrow color picker"
                );
                assert_eq!(
                    button.size.height, input_height,
                    "{id} must match input height"
                );
            }
        })?;
        save(
            &harness.capture()?,
            directory,
            &format!("properties-mesh-color-hsv-{locale}.png"),
        )?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            window.press("escape", cx);
            show(window, "object-property-enabled", cx);
        })?;
        save(
            &harness.capture()?,
            directory,
            &format!("properties-mesh-appearance-{locale}.png"),
        )?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            show(window, "object-property-name", cx);
        })?;
        save(
            &harness.capture()?,
            directory,
            &format!("properties-mesh-identity-{locale}.png"),
        )?;
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
        for (field, value, rejected) in [
            ("maximum", "20", false),
            ("default_value", "15", false),
            ("minimum", "10", false),
            ("minimum", "25", true),
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
                show(window, &format!("property-input-{field}"), cx);
                window.click(format!("property-input-{field}"), cx);
                window.press("cmd-a", cx);
                window.input(value, cx);
                window.press("enter", cx);
                if rejected {
                    harness.view.update(cx, |view, cx| {
                        assert!(view.edit_error.is_some());
                        let doc = view.editor.document().unwrap();
                        let parameter = doc.parameter_draft(&doc.parameters()[0].id).unwrap();
                        assert_eq!(parameter.minimum, 10.);
                        assert_eq!(doc.history_lengths(), (3, 0));
                        view.clear_edit_error(cx);
                    });
                }
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

fn show(window: &mut gpui_kit::Window, field: &str, cx: &mut gpui_kit::App) {
    let mut searched_from_top = false;
    for _ in 0..60 {
        let viewport = window.find("object-properties").bounds();
        let bounds = window
            .try_find(field.to_owned())
            .map(|element| element.bounds());
        if bounds.is_some_and(|b| b.top() >= viewport.top() && b.bottom() <= viewport.bottom()) {
            return;
        }
        if bounds.is_none() && !searched_from_top {
            searched_from_top = true;
            window.scroll(
                "object-properties",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(
                    gpui_kit::px(0.),
                    gpui_kit::px(100_000.),
                )),
                cx,
            );
            window.render_frame(cx);
            continue;
        }
        let delta = if bounds.is_some_and(|b| b.top() < viewport.top()) {
            32.
        } else {
            -32.
        };
        window.scroll(
            "object-properties",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(delta))),
            cx,
        );
        window.render_frame(cx);
    }
}
