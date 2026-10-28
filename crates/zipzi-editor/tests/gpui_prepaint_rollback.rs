//! A failed speculative list prepaint must not publish accessibility nodes.
use gpui_kit::{
    AnyElement, AvailableSpace, Context, Render, Role, TestAppContext, Window, canvas, div,
    prelude::*, px,
};

struct RetryView;

fn child() -> AnyElement {
    div()
        .id("retry-child")
        .role(Role::Button)
        .aria_label("Retried child")
        .size(px(24.))
        .into_any_element()
}

impl Render for RetryView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let canvas = canvas(
            |bounds, window, cx| {
                let space = bounds.size.map(AvailableSpace::Definite);
                let abandoned: Result<(), ()> = window.transact(|window| {
                    child().prepaint_as_root(bounds.origin, space, window, cx);
                    Err(())
                });
                assert!(abandoned.is_err());
                let mut retry = child();
                retry.prepaint_as_root(bounds.origin, space, window, cx);
                retry
            },
            |_, mut child, window, cx| child.paint(window, cx),
        )
        .size_full();
        div()
            .id("retry-parent")
            .role(Role::Group)
            .aria_label("Retry parent")
            .size_full()
            .child(canvas)
    }
}

#[gpui_kit::test]
fn discarded_prepaint_can_retry_the_same_accessibility_node(cx: &mut TestAppContext) {
    // This integration binary has a single test, before any windows exist.
    unsafe { std::env::set_var("GPUI_A11Y_ACTIVE", "1") };
    let (_, cx) = cx.add_window_view(|_, _| RetryView);
    cx.run_until_parked();
    for _ in 0..3 {
        let tree = cx
            .update(|window, cx| {
                window.draw(cx).clear(cx);
                window.debug_a11y_tree_json()
            })
            .unwrap();
        assert_eq!(tree.matches("Retried child").count(), 1, "{tree}");
        assert_eq!(tree.matches("Retry parent").count(), 1, "{tree}");
    }
}
