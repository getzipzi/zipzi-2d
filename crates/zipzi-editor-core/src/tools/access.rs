//! Cached edit destinations and restrictions, independent of pointer hover.
use super::ToolController;
use crate::control_edit::{ControlDescription, ControlEdit};
use crate::{CanvasScene, ComponentRef, Document, ObjectKind, ObjectRef};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};

type Restriction = Arc<str>;

pub(super) struct ObjectAccess {
    pub label: Option<String>,
    pub restriction: Option<Restriction>,
    pub on_edit_only: bool,
    pub selected_ids: HashSet<u32>,
    restricted_points: bool,
}

pub(super) struct EditAccess {
    objects: HashMap<ObjectRef, ObjectAccess>,
    components: HashSet<ComponentRef>,
    selected_restriction: Option<Restriction>,
    point_restrictions: HashMap<ComponentRef, Restriction>,
}

pub(super) struct AccessCache {
    scene: Weak<CanvasScene>,
    revision: u64,
    selection: Vec<ObjectRef>,
    components: Vec<ComponentRef>,
    parameter: Option<ObjectRef>,
    binding: Option<ObjectRef>,
    original_points: bool,
    access: Arc<EditAccess>,
}

impl ToolController {
    pub(super) fn edit_access(
        &self,
        document: &Document,
        scene: &Arc<CanvasScene>,
        selection: &[ObjectRef],
    ) -> Arc<EditAccess> {
        #[cfg(feature = "bench-instrumentation")]
        let _span = zipzi_core::benchmark::span("overlay.access");
        let mut cache = self.access_cache.lock().unwrap();
        let context = document.context();
        if let Some(cached) = cache.as_ref() {
            if cached.scene.as_ptr() == Arc::as_ptr(scene)
                && cached.revision == document.properties_revision()
                && cached.selection == selection
                && cached.components == self.components
                && cached.parameter == context.parameter_focus
                && cached.binding == context.active_binding
                && cached.original_points == self.original_points
            {
                return cached.access.clone();
            }
        }
        let access = Arc::new(EditAccess::build(
            document,
            scene,
            selection,
            &self.components,
            self.original_points,
        ));
        #[cfg(test)]
        self.access_builds
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *cache = Some(AccessCache {
            scene: Arc::downgrade(scene),
            revision: document.properties_revision(),
            selection: selection.to_vec(),
            components: self.components.clone(),
            parameter: context.parameter_focus.clone(),
            binding: context.active_binding.clone(),
            original_points: self.original_points,
            access: access.clone(),
        });
        access
    }
}

impl EditAccess {
    fn build(
        document: &Document,
        scene: &CanvasScene,
        selection: &[ObjectRef],
        selected: &[ComponentRef],
        original_points: bool,
    ) -> Self {
        #[cfg(feature = "bench-instrumentation")]
        let _span = zipzi_core::benchmark::span("overlay.access_build");
        let selection_restriction: Option<Restriction> = document
            .ensure_editable(selection)
            .err()
            .map(|e| Arc::from(e.to_string()));
        let mut objects = HashMap::new();
        let mut weights = HashMap::new();
        for object in selection
            .iter()
            .filter(|r| matches!(r.kind, ObjectKind::Mesh | ObjectKind::Deformer))
        {
            #[cfg(feature = "bench-instrumentation")]
            let _span = zipzi_core::benchmark::span("overlay.object_access");
            let mut state = match ControlEdit::describe(document, scene, object) {
                Ok(ControlDescription::Writable(writable)) => {
                    if let Some(weight) = writable.mesh_weight {
                        weights.insert(object.id.clone(), weight);
                    }
                    ObjectAccess {
                        label: Some(writable.label),
                        restriction: None,
                        on_edit_only: true,
                        selected_ids: HashSet::new(),
                        restricted_points: false,
                    }
                }
                Ok(ControlDescription::ReadOnly(reason)) => ObjectAccess {
                    label: None,
                    restriction: Some(Arc::from(reason)),
                    on_edit_only: true,
                    selected_ids: HashSet::new(),
                    restricted_points: false,
                },
                Err(error) => ObjectAccess {
                    label: None,
                    restriction: Some(Arc::from(error.to_string())),
                    on_edit_only: false,
                    selected_ids: HashSet::new(),
                    restricted_points: false,
                },
            };
            if let Some(reason) = &selection_restriction {
                weights.remove(&object.id);
                state.restriction = Some(reason.clone());
                state.on_edit_only = false;
            }
            objects.insert(object.clone(), state);
        }
        let components: HashSet<_> = selected
            .iter()
            .filter(|c| objects.contains_key(&c.object))
            .cloned()
            .collect();
        for c in &components {
            objects
                .get_mut(&c.object)
                .unwrap()
                .selected_ids
                .insert(c.id);
        }
        let targets: HashSet<_> = components.iter().map(|c| &c.object).collect();
        let selected_restriction = (|| -> anyhow::Result<()> {
            #[cfg(feature = "bench-instrumentation")]
            let _span = zipzi_core::benchmark::span("overlay.selected_restriction");
            for c in selected.iter().filter(|c| components.contains(*c)) {
                if let Some(reason) = &objects[&c.object].restriction {
                    anyhow::bail!("{reason}");
                }
            }
            anyhow::ensure!(
                !targets
                    .iter()
                    .any(|r| scene.ancestors(r).iter().any(|p| targets.contains(p))),
                "Select controls from one deformation level at a time"
            );
            let has_deformer = targets.iter().any(|r| r.kind == ObjectKind::Deformer);
            let has_glue_mesh = targets.iter().any(|r| {
                r.kind == ObjectKind::Mesh
                    && scene
                        .topology
                        .glues
                        .iter()
                        .any(|g| g.mesh_a == r.id || g.mesh_b == r.id)
            });
            anyhow::ensure!(
                !has_deformer || !has_glue_mesh,
                "Edit Glue-connected meshes separately from deformer controls"
            );
            let vertices: Vec<_> = selected
                .iter()
                .filter_map(|c| {
                    weights
                        .get(&c.object.id)
                        .map(|w| ((c.object.id.clone(), c.id), *w))
                })
                .collect();
            scene.glue_map.check_drag(&vertices, original_points)
        })()
        .err()
        .map(|e| Arc::from(e.to_string()));
        let mut point_restrictions = HashMap::new();
        if !original_points {
            #[cfg(feature = "bench-instrumentation")]
            let _span = zipzi_core::benchmark::span("overlay.glue_probes");
            for vertex in scene.glue_map.affected_vertices() {
                let Some(&weight) = weights.get(&vertex.0) else {
                    continue;
                };
                if let Err(error) = scene
                    .glue_map
                    .check_drag(&[(vertex.clone(), weight)], false)
                {
                    objects
                        .get_mut(&ObjectRef::new(ObjectKind::Mesh, &vertex.0))
                        .unwrap()
                        .restricted_points = true;
                    point_restrictions.insert(
                        ComponentRef {
                            object: ObjectRef::new(ObjectKind::Mesh, &vertex.0),
                            id: vertex.1,
                        },
                        Arc::from(error.to_string()),
                    );
                }
            }
        }
        Self {
            objects,
            components,
            selected_restriction,
            point_restrictions,
        }
    }

    pub fn object(&self, object: &ObjectRef) -> Option<&ObjectAccess> {
        self.objects.get(object)
    }
    pub fn uniform_editable(&self, object: &ObjectRef) -> Option<bool> {
        let state = self.objects.get(object)?;
        if state.restriction.is_some() {
            Some(false)
        } else if !state.restricted_points && self.selected_restriction.is_none() {
            Some(true)
        } else {
            None
        }
    }
    pub fn component_selected(&self, component: &ComponentRef) -> bool {
        self.components.contains(component)
    }
    pub fn object_editable(&self, object: &ObjectRef) -> bool {
        self.object_restriction(object).is_none()
    }
    pub fn object_restriction(&self, object: &ObjectRef) -> Option<&str> {
        self.objects.get(object)?.restriction.as_deref()
    }
    pub fn probe(&self, component: &ComponentRef) -> Option<Result<(), &str>> {
        let object = self.objects.get(&component.object)?;
        let restriction = object.restriction.as_deref().or_else(|| {
            if self.components.contains(component) {
                self.selected_restriction.as_deref()
            } else {
                self.point_restrictions.get(component).map(|r| r.as_ref())
            }
        });
        Some(match restriction {
            Some(reason) => Err(reason),
            None => Ok(()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CanvasFrame, Document, Editor, EditorCommand, ViewportId, ViewportKind, ViewportMetrics,
    };
    use std::sync::atomic::Ordering;
    use std::time::Instant;
    use zipzi_core::*;

    fn id(n: u128) -> String {
        uuid::Uuid::from_u128(n).to_string()
    }
    fn fixture() -> anyhow::Result<(Editor, tempfile::TempDir)> {
        let dir = tempfile::tempdir()?;
        let png = dir.path().join("texture.png");
        std::fs::write(&png, include_bytes!("../../tests/fixtures/accent.png"))?;
        let mut sdk =
            zipzi_sdk::AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 1.))?;
        let positions: Vec<_> = (0..100)
            .flat_map(|y| (0..100).map(move |x| Vec2::new(x as f32, y as f32)))
            .collect();
        let triangles: Vec<_> = (0..99_u32)
            .flat_map(|y| {
                (0..99_u32).flat_map(move |x| {
                    let a = y * 100 + x;
                    [[a, a + 1, a + 100], [a + 1, a + 101, a + 100]]
                })
            })
            .collect();
        sdk.edit("Dense bound mesh", None, |edit| {
            edit.create_asset(zipzi_sdk::prepare_png_asset(&id(2), "Texture", &png)?)?;
            edit.create_mesh(Mesh {
                id: id(3),
                runtime_id: "dense".into(),
                name: "Dense".into(),
                texture_asset_id: id(2).into(),
                vertex_ids: (0..10_000).collect(),
                base_positions: positions.clone(),
                uvs: positions
                    .iter()
                    .map(|p| Vec2::new(p.x / 99., p.y / 99.))
                    .collect(),
                triangles,
                ..Default::default()
            })?;
            edit.create_parameter(Parameter {
                id: id(4),
                name: "Pose".into(),
                minimum: 0.,
                maximum: 1.,
                ..Default::default()
            })?;
            edit.create_binding(MeshBinding {
                id: id(5),
                mesh_id: id(3),
                axes: vec![BindingAxis {
                    parameter_id: id(4),
                    keys: vec![0., 1.],
                    ..Default::default()
                }],
                keyforms: vec![
                    MeshKeyform {
                        keys: vec![0.],
                        positions: positions.clone(),
                        ..Default::default()
                    },
                    MeshKeyform {
                        keys: vec![1.],
                        positions: positions.clone(),
                        ..Default::default()
                    },
                ],
            })?;
            Ok(())
        })?;
        let saved = sdk.save_project(&dir.path().join("dense.zipzi"), None)?;
        let mut document = Document::open(&saved.manifest)?;
        document.select(&id(3))?;
        let mut editor = Editor::from_document(document);
        editor.register_primary_viewport(ViewportId::new(), ViewportKind::Modeling)?;
        editor.set_canvas_metrics(ViewportMetrics {
            width: 512.,
            height: 512.,
            scale_factor: 1.,
        });
        Ok((editor, dir))
    }
    fn show(editor: &mut Editor) -> anyhow::Result<CanvasFrame> {
        let frame = editor.preview()?.canvas.unwrap();
        assert!(editor.present_frame(frame.clone()));
        Ok(frame)
    }
    fn builds(editor: &Editor) -> usize {
        editor
            .active_session()
            .unwrap()
            .modeling
            .tools
            .access_builds
            .load(Ordering::Relaxed)
    }

    fn overlays(editor: &Editor) -> usize {
        editor
            .active_session()
            .unwrap()
            .modeling
            .tools
            .overlay_builds
            .load(Ordering::Relaxed)
    }

    #[test]
    fn notices_and_hover_reuse_capabilities_without_building_vertex_geometry() -> anyhow::Result<()>
    {
        let (mut editor, _dir) = fixture()?;
        show(&mut editor)?;
        let before = overlays(&editor);
        assert!(editor.canvas_notice().is_none());
        let access = builds(&editor);
        assert_eq!(overlays(&editor), before);
        let frame = editor
            .displayed_modeling_frame(editor.primary_viewport_binding().unwrap())
            .unwrap()
            .clone();
        let tools = &mut editor.active_session_mut().unwrap().modeling.tools;
        tools.hovered = Some(crate::HitTarget::Component(ComponentRef {
            object: ObjectRef::new(ObjectKind::Mesh, id(3)),
            id: 0,
        }));
        tools.hovered_binding = Some(frame.version.binding);
        assert!(editor.canvas_hover_hint().is_none());
        assert_eq!(builds(&editor), access);
        assert_eq!(overlays(&editor), before);

        editor.execute(EditorCommand::FocusParameter(id(4)))?;
        editor.execute(EditorCommand::SetPreviewParameter {
            parameter_id: id(4),
            value: 0.5,
        })?;
        show(&mut editor)?;
        assert!(
            editor.canvas_notice().is_none(),
            "off-key restrictions are shown on edit only"
        );
        let tools = &mut editor.active_session_mut().unwrap().modeling.tools;
        tools.hovered = Some(crate::HitTarget::Component(ComponentRef {
            object: ObjectRef::new(ObjectKind::Mesh, id(3)),
            id: 0,
        }));
        tools.hovered_binding = Some(frame.version.binding);
        assert!(editor.canvas_hover_hint().unwrap().contains("Pose"));
        assert_eq!(overlays(&editor), before);
        let overlay = editor.overlay();
        assert!(overlay.vertices.iter().all(|v| !v.editable));
        assert_eq!(editor.canvas_hover_hint(), overlay.point_unavailable);

        editor.execute(EditorCommand::SetObjectsLocked {
            objects: vec![ObjectRef::new(ObjectKind::Mesh, id(3))],
            locked: true,
        })?;
        let before = overlays(&editor);
        let notice = editor.canvas_notice().unwrap();
        assert!(notice.contains("locked"));
        assert_eq!(overlays(&editor), before);
        assert_eq!(editor.overlay().unavailable, Some(notice));
        editor.execute(EditorCommand::Undo)?;
        assert!(editor.canvas_notice().is_none());
        Ok(())
    }

    #[cfg(feature = "bench-instrumentation")]
    #[test]
    fn mesh_capability_description_and_drag_weight_share_one_blend_query() -> anyhow::Result<()> {
        let (mut editor, _dir) = fixture()?;
        show(&mut editor)?;
        let capture = zipzi_core::benchmark::Capture::start();
        assert!(editor.canvas_notice().is_none());
        assert!(editor.overlay().vertices.iter().all(|v| v.editable));
        let stages = capture.finish();
        assert_eq!(stages["overlay.mesh_blend_target"].calls, 1);
        assert_eq!(stages["overlay.access_build"].calls, 1);
        Ok(())
    }

    #[test]
    fn dense_mesh_hover_reuses_capabilities_and_updates_on_edit_context_changes()
    -> anyhow::Result<()> {
        let (mut editor, _dir) = fixture()?;
        let frame = show(&mut editor)?;
        let before = builds(&editor);
        let start = Instant::now();
        let overlay = editor.overlay();
        let cold = start.elapsed();
        assert_eq!(overlay.vertices.len(), 10_000);
        assert!(overlay.vertices.iter().all(|v| v.editable));
        assert_eq!(builds(&editor), before + 1);
        let start = Instant::now();
        for i in 0..100 {
            let tools = &mut editor.active_session_mut().unwrap().modeling.tools;
            tools.hovered = Some(crate::HitTarget::Component(ComponentRef {
                object: ObjectRef::new(ObjectKind::Mesh, id(3)),
                id: i,
            }));
            tools.hovered_binding = Some(frame.version.binding);
            std::hint::black_box(editor.overlay());
        }
        eprintln!(
            "10,000 vertices: cold overlay {:?}, 100 hover overlays {:?}; one capability build",
            cold,
            start.elapsed()
        );
        assert_eq!(
            builds(&editor),
            before + 1,
            "hover must not recompute permissions"
        );
        editor.execute(EditorCommand::FocusParameter(id(4)))?;
        assert!(editor.overlay().vertices.iter().all(|v| v.editable));
        assert_eq!(
            builds(&editor),
            before + 2,
            "focus can choose a different write destination"
        );
        editor.execute(EditorCommand::SetPreviewParameter {
            parameter_id: id(4),
            value: 0.5,
        })?;
        show(&mut editor)?;
        let off_key = editor.overlay();
        assert!(off_key.vertices.iter().all(|v| !v.editable));
        assert!(off_key.unavailable.as_deref().unwrap().contains("Pose"));
        let blocked_builds = builds(&editor);
        editor.overlay();
        assert_eq!(builds(&editor), blocked_builds);
        editor.execute(EditorCommand::SetPreviewParameter {
            parameter_id: id(4),
            value: 1.,
        })?;
        show(&mut editor)?;
        assert!(editor.overlay().vertices.iter().all(|v| v.editable));
        editor.execute(EditorCommand::SetObjectsLocked {
            objects: vec![ObjectRef::new(ObjectKind::Mesh, id(3))],
            locked: true,
        })?;
        let locked = editor.overlay();
        assert_eq!(locked.vertices.len(), 10_000);
        assert!(locked.vertices.iter().all(|v| !v.editable));
        editor.execute(EditorCommand::Undo)?;
        assert!(editor.overlay().vertices.iter().all(|v| v.editable));
        Ok(())
    }
}
