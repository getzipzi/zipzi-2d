//! Animation content and navigation. This viewport has no Modeling tool handler.
use super::*;

pub struct AnimationViewport {
    pub(crate) surface: Entity<ViewportSurface>,
    _observe: Subscription,
    header: Option<zoom_bar::HeaderState>,
}
impl AnimationViewport {
    pub fn new(
        owner: WeakEntity<EditorView>,
        viewport: zipzi_editor_core::ViewportId,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let surface = ViewportSurface::new(
            owner,
            viewport,
            zipzi_editor_core::ViewportKind::Animation,
            Self::content,
            |_, _, _, _| {},
            toolbar::ToolbarRegistry::navigation(),
            window,
            cx,
        );
        cx.new(|cx| Self {
            _observe: cx.observe_in(&surface, window, |this: &mut Self, surface, window, cx| {
                let next = ViewportSurface::header_state(&surface, window, cx);
                if this.header != next {
                    this.header = next;
                    cx.notify();
                }
            }),
            header: None,
            surface,
        })
    }
    fn content(owner: &EditorView, viewport: zipzi_editor_core::ViewportId) -> SurfaceContent {
        // The shared image accessor also admits this document's frozen image
        // while saving, when core deliberately has no live viewport binding.
        let frame = owner.animation_frame_for(viewport);
        let mut content = SurfaceContent::navigation(
            frame.map(|f| (f.image.clone(), f.state.transform.metrics)),
            owner.viewport_error(viewport).cloned().or_else(|| {
                owner.editor.animation().and_then(|a| {
                    a.error
                        .as_ref()
                        .map(crate::panels::animation_feedback::failure_text)
                })
            }),
        );
        if owner.editor.cursor_physics_active() {
            content.hand_tool = false;
            content.controls = Some(Self::cursor_preview_hint);
        }
        content
    }

    fn cursor_preview_hint(
        surface: &ViewportSurface,
        modal: bool,
        cx: &mut Context<ViewportSurface>,
    ) -> AnyElement {
        let has_bindings = surface.owner.upgrade().is_some_and(|owner| {
            owner
                .read(cx)
                .editor
                .animation()
                .is_some_and(|a| a.has_cursor_preview_bindings())
        });
        div()
            .id("cursor-preview-hint")
            .test_support()
            .absolute()
            .bottom_2()
            .left_2()
            .right_2()
            .flex()
            .justify_center()
            .when(!modal, |hint| {
                hint.child(
                    div()
                        .px_2()
                        .py_1()
                        .rounded_sm()
                        .bg(cx.theme().background)
                        .text_color(cx.theme().foreground)
                        .text_xs()
                        .child(if has_bindings {
                            t!("physics.cursor_hint")
                        } else {
                            t!("physics.cursor_no_mapping")
                        }),
                )
            })
            .into_any_element()
    }
}
viewport_panel!(AnimationViewport, "dock.animation_viewport", 204.);
