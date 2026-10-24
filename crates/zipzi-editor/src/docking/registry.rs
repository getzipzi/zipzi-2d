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
        viewport_slots: [zipzi_editor_core::ViewportId; 2],
        thumbnails: Entity<crate::mesh_thumbnails::MeshThumbnails>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let modeling =
            crate::viewports::ModelingViewport::new(owner.clone(), viewport_slots[0], window, cx);
        let animation =
            crate::viewports::AnimationViewport::new(owner.clone(), viewport_slots[1], window, cx);
        let timeline = crate::panels::timeline::Timeline::new(owner.clone(), window, cx);
        let properties = crate::panels::PropertiesPanel::new(owner.clone(), window, cx);
        let mesh_tree =
            crate::panels::MeshTreePanel::new(owner.clone(), thumbnails.clone(), window, cx);
        let deformer_tree =
            crate::panels::DeformerTreePanel::new(owner.clone(), thumbnails, window, cx);
        let parameters = crate::panels::ParametersPanel::new(owner.clone(), window, cx);
        use crate::panels::animation_panel::AnimationPanel;
        let animation_motions =
            AnimationPanel::new(owner.clone(), PanelKind::AnimationMotions, window, cx);
        let animation_expressions =
            AnimationPanel::new(owner.clone(), PanelKind::AnimationExpressions, window, cx);

        let animation_inspectors: HashMap<_, _> =
            [PanelKind::AnimationPhysics, PanelKind::AnimationPose]
                .into_iter()
                .map(|kind| (kind, AnimationPanel::new(owner.clone(), kind, window, cx)))
                .collect();
        let mut panels: PanelViews = animation_inspectors
            .iter()
            .map(|(kind, panel)| (*kind, panel_handle(panel.clone())))
            .collect();
        for (kind, handle) in [
            (PanelKind::ModelingViewport, panel_handle(modeling.clone())),
            (
                PanelKind::AnimationViewport,
                panel_handle(animation.clone()),
            ),
            (PanelKind::AnimationTimeline, panel_handle(timeline.clone())),
            (
                PanelKind::AnimationMotions,
                panel_handle(animation_motions.clone()),
            ),
            (
                PanelKind::AnimationExpressions,
                panel_handle(animation_expressions.clone()),
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
            &modeling,
            window,
            cx,
            crate::viewports::ModelingViewport::header_state,
        );
        header::register_with_state(
            &mut headers,
            &animation,
            window,
            cx,
            crate::viewports::AnimationViewport::header_state,
        );
        header::register(&mut headers, &mesh_tree, cx);
        header::register(&mut headers, &deformer_tree, cx);
        header::register(&mut headers, &parameters, cx);
        header::register_with_state(
            &mut headers,
            &properties,
            window,
            cx,
            crate::panels::PropertiesPanel::header_state,
        );
        header::register(&mut headers, &timeline, cx);
        header::register(&mut headers, &animation_motions, cx);
        header::register(&mut headers, &animation_expressions, cx);
        header::register(
            &mut headers,
            &animation_inspectors[&PanelKind::AnimationPhysics],
            cx,
        );
        header::register(
            &mut headers,
            &animation_inspectors[&PanelKind::AnimationPose],
            cx,
        );
        let headers = Rc::new(headers);
        let input_regions: HashMap<_, crate::input_routing::InputRegion> = panels
            .keys()
            .copied()
            .filter(|kind| !kind.is_viewport() && *kind != PanelKind::AnimationTimeline)
            .map(|kind| {
                (
                    kind,
                    if kind == PanelKind::Properties {
                        crate::panels::PropertiesPanel::input_region(&properties, cx)
                    } else {
                        crate::input_routing::InputRegion::panel(
                            panels[&kind].focus_handle(cx),
                            kind,
                        )
                    },
                )
            })
            .collect();
        let regions = input_regions
            .iter()
            .map(|(kind, region)| (panels[kind].view().entity_id(), region.clone()))
            .collect();
        let area = cx.new(|cx| {
            let skin = DockSkin::new(cx).with_resize_handle_line(false);
            DockArea::new(
                "zipzi-workspace",
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
                regions,
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
            input_regions,
            area,
            viewports: viewports::ViewportPanels::new(modeling, animation, cx),
            timeline_id: timeline.read(cx).id,
            timeline_hints: timeline.read(cx).hints.clone(),
            timeline,
            animation_motions,
            animation_expressions,
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
