//! Per-window panel creation, headers and dock factories.
use super::*;
use std::rc::Weak;

// Factories resolve against the requesting area, never the last-created window.
// Weak ownership keeps the global registry from retaining closed editors.
#[derive(Default)]
struct WorkspacePanels(HashMap<EntityId, Weak<PanelViews>>);
impl Global for WorkspacePanels {}

impl DockWorkspace {
    pub fn new(
        workspace: WorkspaceKind,
        owner: WeakEntity<EditorView>,
        thumbnails: Entity<crate::mesh_thumbnails::MeshThumbnails>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let canvas = crate::canvas::CanvasView::new(owner.clone(), window, cx);
        let timeline = crate::panels::timeline::Timeline::new(owner.clone(), window, cx);
        let properties = crate::panels::PropertiesPanel::new(owner.clone(), window, cx);
        let mesh_tree =
            crate::panels::MeshTreePanel::new(owner.clone(), thumbnails.clone(), window, cx);
        let deformer_tree =
            crate::panels::DeformerTreePanel::new(owner.clone(), thumbnails, window, cx);
        let parameters = crate::panels::ParametersPanel::new(owner.clone(), window, cx);
        use crate::panels::animation_panel::AnimationPanel;
        let animation_motions = AnimationPanel::new(owner.clone(), PanelKind::AnimationMotions, cx);
        let animation_expressions =
            AnimationPanel::new(owner.clone(), PanelKind::AnimationExpressions, cx);
        let animation_info = AnimationPanel::new(owner.clone(), PanelKind::AnimationInfo, cx);
        let animation_parameters =
            AnimationPanel::new(owner.clone(), PanelKind::AnimationParameters, cx);

        let animation_inspectors: HashMap<_, _> = [
            PanelKind::AnimationProperties,
            PanelKind::AnimationPhysics,
            PanelKind::AnimationPose,
        ]
        .into_iter()
        .map(|kind| (kind, AnimationPanel::new(owner.clone(), kind, cx)))
        .collect();
        let mut panels: PanelViews = animation_inspectors
            .iter()
            .map(|(kind, panel)| (*kind, panel_handle(panel.clone())))
            .collect();
        for (kind, handle) in [
            (PanelKind::Canvas, panel_handle(canvas.clone())),
            (PanelKind::AnimationTimeline, panel_handle(timeline.clone())),
            (
                PanelKind::AnimationMotions,
                panel_handle(animation_motions.clone()),
            ),
            (
                PanelKind::AnimationExpressions,
                panel_handle(animation_expressions.clone()),
            ),
            (
                PanelKind::AnimationInfo,
                panel_handle(animation_info.clone()),
            ),
            (
                PanelKind::AnimationParameters,
                panel_handle(animation_parameters.clone()),
            ),
            (PanelKind::Properties, panel_handle(properties.clone())),
            (PanelKind::MeshTree, panel_handle(mesh_tree.clone())),
            (PanelKind::Deformers, panel_handle(deformer_tree.clone())),
            (PanelKind::Parameters, panel_handle(parameters.clone())),
        ] {
            panels.insert(kind, handle);
        }

        let mut document_panels = std::collections::HashMap::new();
        for kind in PanelKind::DOCUMENT_PANELS {
            let panel = crate::panels::DocumentPanel::new(owner.clone(), kind, window, cx);
            let handle = panel_handle(panel.clone());
            panels.insert(kind, handle);
            document_panels.insert(kind, panel);
        }
        let mut headers = header::PanelHeaders::new();
        header::register_with_state(
            &mut headers,
            &canvas,
            window,
            cx,
            crate::canvas::CanvasView::header_state,
        );
        header::register(&mut headers, &mesh_tree, cx);
        header::register(&mut headers, &deformer_tree, cx);
        header::register(&mut headers, &parameters, cx);
        header::register(&mut headers, &timeline, cx);
        header::register(&mut headers, &animation_motions, cx);
        header::register(&mut headers, &animation_expressions, cx);
        header::register(&mut headers, &animation_info, cx);
        header::register(
            &mut headers,
            &document_panels[&editor_core::PanelKind::Relations],
            cx,
        );
        let headers = Rc::new(headers);
        let area = cx.new(|cx| {
            let skin = DockSkin::new(cx);
            DockArea::new(
                "kasane-workspace",
                Some(layout_version(workspace)),
                window,
                cx,
            )
            .with_renderer(skin::EditorDockSkin::new(
                skin,
                parameters.entity_id(),
                headers.clone(),
                cx.weak_entity(),
                owner.clone(),
            ))
        });
        let panels = Rc::new(panels);
        if cx.try_global::<WorkspacePanels>().is_none() {
            cx.set_global(WorkspacePanels::default());
        }
        let registry = &mut cx.global_mut::<WorkspacePanels>().0;
        registry.retain(|_, views| views.strong_count() > 0);
        registry.insert(area.entity_id(), Rc::downgrade(&panels));
        for spec in PANELS {
            let kind = spec.kind;
            register_panel(cx, spec.name, move |context, _, cx| {
                cx.global::<WorkspacePanels>().0[&context.dock_area().entity_id()]
                    .upgrade()
                    .expect("live editor panel catalog")[&kind]
                    .clone()
            });
        }
        let last_valid = area.read(cx).dump(cx);
        #[cfg(test)]
        let (writer, errors) =
            LayoutWriter::new_sync(cx.global::<settings::Settings>().store.clone());
        #[cfg(not(test))]
        let (writer, errors) = LayoutWriter::new(cx.global::<settings::Settings>().store.clone());

        let mut result = Self {
            area,
            canvases: canvases::CanvasPanels::new(canvas),
            timeline,
            animation_motions,
            animation_expressions,
            animation_info,
            animation_parameters,
            animation_inspectors,
            properties,
            mesh_tree,
            deformer_tree,
            parameters,
            document_panels,
            panels,
            last_valid,
            pending_layout_measurement: false,
            workspace,
            layouts: HashMap::new(),
            layout_epoch: 0,
            writer,
            errors: Some(errors),
        };
        result.reset(window, cx);
        result
    }
}
