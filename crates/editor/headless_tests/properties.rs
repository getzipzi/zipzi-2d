use super::{document_fixture, setup, setup_with_document};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

#[gpui_kit::test]
fn mesh_properties_are_read_only_and_document_navigation_preserves_history(
    cx: &mut TestAppContext,
) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("object-property-name", cx);
        assert!(window.try_find("object-property-input").is_none());
        assert!(window.try_find("apply-object-property").is_none());
        assert!(window.try_find("cancel-object-property").is_none());
        window.click("inspect-document", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.context().inspected.as_ref().unwrap().kind,
            editor_core::ObjectKind::Document
        );
        assert_eq!(view.editor.selected_layer().unwrap().name, "Diamond");
        assert_eq!(doc.history_lengths(), (0, 0));
        assert!(!doc.modified());
    });
}

#[gpui_kit::test]
fn part_properties_use_the_same_read_only_inspector(cx: &mut TestAppContext) {
    let (mut document, assets) =
        document_fixture::sample_document_with_nonuniform_parameter_keys().unwrap();
    let part = document.object_index().bindings[0].target.clone();
    document.select_object(part.clone()).unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("object-property-name", cx);
        assert!(window.try_find("object-property-input").is_none());
        assert!(window.try_find("apply-object-property").is_none());
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.context().inspected.as_ref(), Some(&part));
        assert_eq!(doc.history_lengths(), (0, 0));
        assert!(!doc.modified());
    });
}

#[gpui_kit::test]
fn compact_properties_align_and_wrap_without_overflow_in_narrow_panels(cx: &mut TestAppContext) {
    use gpui_kit::{prelude::*, *};

    struct PropertyHarness(Pixels);
    impl Render for PropertyHarness {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            crate::ui::compact::property_row(
                "canvas.pixels_per_unit",
                crate::ui::compact::property_value(
                    "a-long-identifier-01234567-89ab-cdef-0123-456789abcdef",
                ),
                cx,
            )
            .id("property-row")
            .test_support()
            .w(self.0)
        }
    }

    cx.update(gpui_kit::init);
    let (window, harness) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|_| PropertyHarness(px(360.)))
        })
        .unwrap()
    });
    for width in [360., 180., 100.] {
        cx.update_window(window, |_, window, cx| {
            harness.update(cx, |harness, cx| {
                harness.0 = px(width);
                cx.notify();
            });
            window.render_frame(cx);
            let row = window.find("property-row").bounds();
            let label = window
                .find("property-label-canvas.pixels_per_unit")
                .bounds();
            let value = window.find("property-value").bounds();
            for bounds in [label, value] {
                assert!(bounds.left() >= row.left());
                assert!(bounds.right() <= row.right());
                assert!(bounds.bottom() <= row.bottom());
            }
            if width == 360. {
                assert!(value.left() >= label.right());
                assert!(value.top() < label.bottom());
            } else {
                assert!(value.top() >= label.bottom());
            }
        })
        .unwrap();
    }
}
