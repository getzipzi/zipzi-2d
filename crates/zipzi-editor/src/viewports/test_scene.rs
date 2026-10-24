//! Test-only simultaneous panels, using the production surfaces and render workers.
//! Product workspaces continue to contain their single declared primary viewport.
use super::*;
use zipzi_editor_core::{ViewportId, ViewportKind};

pub(crate) struct ViewportScene {
    pub panels: Vec<AnyView>,
    pub surfaces: Vec<Entity<ViewportSurface>>,
    pub instances: Vec<(ViewportId, ViewportKind)>,
}
impl EditorView {
    pub(crate) fn install_viewport_scene(
        &mut self,
        kinds: &[ViewportKind],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<ViewportId> {
        self.focused_viewport = None;
        self.input_router.menu_target.set(None);
        if let Some(previous) = self.viewport_scene.take() {
            for (id, _) in previous.instances {
                self.editor.unregister_viewport(id);
                self.viewport_frames.remove(&id);
                if let Some(requests) = &self.requests {
                    requests.cancel(id);
                }
            }
        }
        let mut scene = ViewportScene {
            panels: Vec::new(),
            surfaces: Vec::new(),
            instances: Vec::new(),
        };
        for &kind in kinds {
            let id = ViewportId::new();
            let (panel, surface) = match kind {
                ViewportKind::Modeling => {
                    let panel = ModelingViewport::new(cx.weak_entity(), id, window, cx);
                    (AnyView::from(panel.clone()), panel.read(cx).surface.clone())
                }
                ViewportKind::Animation => {
                    let panel = AnimationViewport::new(cx.weak_entity(), id, window, cx);
                    (AnyView::from(panel.clone()), panel.read(cx).surface.clone())
                }
            };
            self.editor.register_viewport(id, kind).unwrap();
            scene.panels.push(panel);
            scene.surfaces.push(surface);
            scene.instances.push((id, kind));
        }
        let ids = scene.instances.iter().map(|(id, _)| *id).collect();
        self.viewport_scene = Some(scene);
        self.sync_viewport_visibility(window, cx);
        self.redraw(cx);
        ids
    }
}
