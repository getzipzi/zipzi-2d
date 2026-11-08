//! Shared selection transforms for drawable objects and organization containers.
use crate::control_edit::{ControlChange, ControlEdit};
use crate::*;
use anyhow::{Result, ensure};

pub(crate) fn members(scene: &CanvasScene, selection: &[ObjectRef]) -> Vec<ObjectRef> {
    let mut targets = Vec::new();
    for object in selection {
        match object.kind {
            ObjectKind::Mesh | ObjectKind::Deformer => targets.push(object.clone()),
            ObjectKind::Part | ObjectKind::Offscreen => {
                let part = if object.kind == ObjectKind::Offscreen {
                    scene
                        .index
                        .get(object)
                        .map(|i| ObjectRef::new(ObjectKind::Part, &i.part_id))
                } else {
                    Some(object.clone())
                };
                if let Some(part) = part {
                    targets.extend(
                        scene
                            .controls
                            .iter()
                            .filter(|c| {
                                c.kind != ControlKind::Glue
                                    && scene.ancestors(&c.object).contains(&part)
                            })
                            .map(|c| c.object.clone()),
                    );
                    targets.extend(
                        scene
                            .meshes
                            .iter()
                            .map(|m| ObjectRef::new(ObjectKind::Mesh, &m.id))
                            .filter(|m| scene.ancestors(m).contains(&part)),
                    );
                }
            }
            _ => {}
        }
    }
    // Filter before root normalization: an unavailable ancestor must not suppress
    // an independently visible descendant or become an invisible write target.
    targets.retain(|o| {
        scene.in_isolation_scope(o)
            && match o.kind {
                ObjectKind::Mesh => scene.mesh_visible(&o.id),
                ObjectKind::Deformer => {
                    scene.control(&o.id).is_some_and(|c| c.enabled && c.visible)
                }
                _ => false,
            }
    });
    let mut seen = std::collections::HashSet::new();
    targets.retain(|o| seen.insert(o.clone()));
    targets
}

fn roots(scene: &CanvasScene, mut targets: Vec<ObjectRef>) -> Vec<ObjectRef> {
    let all = targets.clone();
    let mut seen = std::collections::HashSet::new();
    targets
        .retain(|o| !scene.ancestors(o).iter().any(|a| all.contains(a)) && seen.insert(o.clone()));
    targets
}

pub(crate) fn point(scene: &CanvasScene, component: &ComponentRef) -> Option<ModelPoint> {
    if component.object.kind == ObjectKind::Mesh {
        return scene
            .mesh(&component.object.id)?
            .vertices
            .iter()
            .find(|(id, _)| *id == component.id)
            .map(|(_, p)| *p);
    }
    scene
        .warp_selected_points(std::slice::from_ref(component))
        .first()
        .map(|(_, p)| *p)
}

impl crate::tools::ToolController {
    pub(crate) fn object_selection_frame(
        &self,
        document: &Document,
        frame: &CanvasFrame,
    ) -> Option<crate::deformer::WarpGizmos> {
        // Rotation has its own origin/axis/ring controls. A directly selected
        // Rotation never owns or participates in the generic object frame.
        let selection: Vec<_> = document
            .context()
            .selections
            .iter()
            .filter(|o| {
                !frame
                    .geometry
                    .control(&o.id)
                    .is_some_and(|c| c.kind == ControlKind::Rotation)
            })
            .cloned()
            .collect();
        // Preserve the existing Warp controller/selection semantics.
        if selection.iter().all(|o| {
            o.kind == ObjectKind::Deformer
                && frame
                    .geometry
                    .control(&o.id)
                    .is_some_and(|c| c.kind == ControlKind::Warp)
        }) {
            return None;
        }
        let scene = &frame.geometry;
        let mut objects = members(scene, &selection);
        if objects.is_empty() {
            return None;
        }
        let components: Vec<_> = self
            .components
            .iter()
            .filter(|c| objects.contains(&c.object))
            .filter_map(|c| point(scene, c))
            .collect();
        // A single point keeps the whole-object frame as a separate operation
        // entrypoint. Direct point dragging and keyboard nudges retain their
        // component selection independently of this frame's scope.
        let local = components.len() > 1;
        if local {
            objects.retain(|o| self.components.iter().any(|c| c.object == *o));
        } else {
            objects = roots(scene, objects);
        }
        let points = if local {
            components
        } else {
            objects
                .iter()
                .flat_map(|o| {
                    if let Some(mesh) = scene.mesh(&o.id).filter(|_| o.kind == ObjectKind::Mesh) {
                        mesh.vertices.iter().map(|(_, p)| *p).collect::<Vec<_>>()
                    } else {
                        let meshes = scene.member_meshes(o);
                        if !meshes.is_empty() {
                            meshes
                                .iter()
                                .flat_map(|m| m.vertices.iter().map(|(_, p)| *p))
                                .collect()
                        } else {
                            scene
                                .control(&o.id)
                                .map(|c| c.points.clone())
                                .unwrap_or_default()
                        }
                    }
                })
                .collect()
        };
        let bounds = crate::picking::bounds(points.into_iter().map(|p| {
            let p = frame.transform.to_viewport(p);
            ModelPoint { x: p.x, y: p.y }
        }))?;
        Some(crate::deformer::WarpGizmos {
            local,
            objects,
            items: vec![(selection.first()?.clone(), DeformerGizmo::object(bounds))],
            adjusting: vec![],
        })
    }
}

impl Editor {
    /// A key press moves the current points or selected object roots in canvas units.
    pub fn nudge_selection(&mut self, binding: ViewportBinding, delta: ModelVector) -> Result<()> {
        ensure!(
            self.viewport_binding_valid(binding) && binding.kind == ViewportKind::Modeling,
            "Modeling viewport changed"
        );
        ensure!(!self.gesture_active(), "Finish the current gesture first");
        ensure!(
            !self.manual_for_viewport(binding.viewport),
            "Finish mesh topology editing first"
        );
        ensure!(
            delta.x.is_finite() && delta.y.is_finite(),
            "Invalid displacement"
        );
        self.sync_runtime()?;
        let session = self
            .active_session_mut()
            .ok_or_else(|| anyhow::anyhow!("Document closed"))?;
        let document = &mut session.document;
        document.finish_edit()?;
        let snapshot = document.snapshot()?;
        let scene = document.geometry(&snapshot);
        let (_, scene) = self.project_modeling_scene(binding, snapshot, scene)?;
        let session = self.active_session_mut().unwrap();
        let document = &mut session.document;
        let selection = document.context().selections.clone();
        document.ensure_editable(&selection)?;
        let members = members(&scene, &selection);
        let components: Vec<_> = session
            .modeling
            .tools
            .components
            .iter()
            .filter(|c| members.contains(&c.object))
            .cloned()
            .collect();
        let local = !components.is_empty();
        let objects = if local {
            members
                .into_iter()
                .filter(|o| components.iter().any(|c| c.object == *o))
                .collect()
        } else {
            roots(&scene, members)
        };
        ensure!(!objects.is_empty(), "Select an editable canvas object");
        let mut edits = ControlEdit::prepare_targets(
            document,
            &scene,
            &objects,
            session.modeling.tools.original_points,
        )?;
        for target in &mut edits {
            target.point_weights = session
                .modeling
                .tools
                .component_weights
                .iter()
                .filter(|(c, _)| c.object == target.object)
                .map(|(c, w)| (c.id, *w))
                .collect();
        }
        document.begin_control_edit(&edits)?;
        let change = if local {
            ControlChange::Translate(delta)
        } else {
            ControlChange::MoveAll(delta)
        };
        let result = document
            .preview_control_edit(&edits, &components, change)
            .and_then(|_| document.finish_edit());
        if result.is_err() {
            document.cancel_edit();
        }
        session.invalidate_scene();
        result
    }
}
