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
        viewport: editor_core::ViewportId,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let surface = ViewportSurface::new(
            owner,
            viewport,
            editor_core::ViewportKind::Animation,
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
    fn content(owner: &EditorView, viewport: editor_core::ViewportId) -> SurfaceContent {
        // The shared image accessor also admits this document's frozen image
        // while saving, when core deliberately has no live viewport binding.
        let frame = owner.animation_frame_for(viewport);
        SurfaceContent {
            image: frame.map(|f| (f.image.clone(), f.state.transform.metrics)),
            overlay: Default::default(),
            error: owner.viewport_error(viewport).cloned().or_else(|| {
                owner.editor.animation().and_then(|a| {
                    a.error
                        .as_ref()
                        .map(crate::panels::animation_feedback::failure_text)
                })
            }),
            hand_tool: true,
            controls: None,
            cursor: None,
            interaction_label: None,
            target_label: None,
            vertex_notice: None,
        }
    }
}
viewport_panel!(AnimationViewport, "dock.animation_viewport", 176.);
