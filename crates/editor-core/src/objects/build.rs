//! Build the immutable panel projection from the authoring document.
use super::bindings::grid_points;
use super::properties::deformer_properties;
use super::{BindingInfo, ObjectIndex, ObjectInfo, ObjectKind, ObjectRef, Relation, RelationKind};
use kasane_core::document::{
    CdiParameterEntry, ExpressionTarget, ModelTargetRef, MotionTrackTarget, PosePartRef,
};
use kasane_core::{BindingAxis, BlendShapeTargetKind, Document as Model, SceneTrack};
use serde_json::{Value, json};

impl ObjectIndex {
    pub fn add(
        &mut self,
        r: ObjectRef,
        name: impl Into<String>,
        properties: Value,
        enabled: Option<bool>,
    ) {
        let name = name.into();
        std::sync::Arc::make_mut(&mut self.structure)
            .slots
            .insert(r.clone(), self.objects.len());
        self.objects.push(ObjectInfo {
            name: if name.is_empty() { r.id.clone() } else { name },
            reference: r,
            properties,
            diagnostic: None,
            enabled,
        });
    }
    fn edge(
        &mut self,
        source: &ObjectRef,
        kind: RelationKind,
        target: ObjectRef,
        detail: impl Into<String>,
    ) {
        let i = self.relations.len();
        let structure = std::sync::Arc::make_mut(&mut self.structure);
        structure
            .outgoing
            .entry(source.clone())
            .or_default()
            .push(i);
        structure
            .incoming
            .entry(target.clone())
            .or_default()
            .push(i);
        self.relations.push(Relation {
            source: source.clone(),
            target,
            kind,
            detail: detail.into(),
        });
    }
    fn ref_edge(
        &mut self,
        source: &ObjectRef,
        kind: RelationKind,
        target_kind: ObjectKind,
        id: &str,
    ) {
        if !id.is_empty() {
            self.edge(source, kind, ObjectRef::new(target_kind, id), "");
        }
    }
    fn unresolved(&mut self, category: &str, runtime: &str) -> ObjectRef {
        let r = ObjectRef::new(ObjectKind::Unresolved, format!("{category}:{runtime}"));
        if self.get(&r).is_none() {
            self.add(
                r.clone(),
                runtime,
                json!({"category":category,"runtime_id":runtime}),
                None,
            );
            self.objects.last_mut().unwrap().diagnostic =
                Some("Unresolved reference; original ID retained".into());
        }
        r
    }
    fn add_binding(&mut self, info: BindingInfo) {
        self.add(info.reference.clone(), format!("{} · {}", info.target.id, info.layout()), json!({"axes": info.axes, "keyforms": info.points.len(), "read_only": !info.writable()}), None);
        for axis in &info.axes {
            self.edge(
                &ObjectRef::new(ObjectKind::Parameter, &axis.parameter_id),
                RelationKind::ParameterAxis,
                info.reference.clone(),
                format!("{} keys", axis.keys.len()),
            );
            let p = std::sync::Arc::make_mut(&mut self.structure)
                .parameters
                .entry(axis.parameter_id.clone())
                .or_default();
            if info.blend {
                p.blend.push(info.reference.clone());
            } else {
                p.regular.push(info.reference.clone());
            }
        }
        self.edge(
            &info.reference,
            if info.blend {
                RelationKind::BlendTarget
            } else {
                RelationKind::BindingTarget
            },
            info.target.clone(),
            "",
        );
        std::sync::Arc::make_mut(&mut self.structure)
            .binding_slots
            .insert(info.reference.clone(), self.bindings.len());
        self.bindings.push(info);
    }
    pub fn build(m: &Model) -> Self {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = kasane_core::benchmark::span("editor.object_index_build");
        use ObjectKind as K;
        use RelationKind as R;
        let mut s = Self {
            draw_groups: kasane_core::draw_order::resolved_groups(m).into(),
            ..Default::default()
        };
        let doc = ObjectRef::new(K::Document, m.id());
        s.add(doc.clone(), "Document", json!({"export_settings":m.export_settings(),"source_canvas":m.source_canvas(),"model_settings":m.model3_settings(),"display_info":m.display_info(),"missing_attachments":m.missing_attachments(),"draw_groups":m.draw_order_groups()}), None);
        for id in m.part_order() {
            if let Some(x) = m.get_part(id) {
                let r = ObjectRef::new(K::Part, id);
                s.add(r.clone(), &x.name, json!(x), Some(x.enabled));
                s.ref_edge(&r, R::PartOwner, K::Part, &x.parent_id);
            }
        }
        for id in m.transform_order() {
            if let Some(x) = m.get_transform(id) {
                let r = ObjectRef::new(K::Deformer, id);
                s.add(r.clone(), &x.name, deformer_properties(x), Some(x.enabled));
                s.ref_edge(&r, R::PartOwner, K::Part, x.part());
                s.ref_edge(&r, R::DeformParent, K::Deformer, x.parent());
            }
        }
        for id in m.mesh_order() {
            if let Some(x) = m.get_mesh(id) {
                let r = ObjectRef::new(K::Mesh, id);
                s.add(r.clone(),&x.name,json!({"id":x.id,"name":x.name,"runtime_id":x.runtime_id,"part_id":x.part_id,"deformer_id":x.deformer_id,"texture_asset_id":x.texture_asset_id,"appearance":x.appearance,"draw_order":x.draw_order,"blend_mode":x.blend_mode,"enabled":x.enabled,"double_sided":x.double_sided,"inverted_mask":x.inverted_mask,"masks":x.masks,"vertices":x.vertex_ids.len(),"triangles":x.triangles.len()}),Some(x.enabled));
                let slot = *s.structure.slots.get(&r).unwrap();
                crate::property::mesh::inspector_fields(x, &mut s.objects[slot].properties);
                s.ref_edge(&r, R::PartOwner, K::Part, &x.part_id);
                s.ref_edge(&r, R::DeformParent, K::Deformer, &x.deformer_id);
                s.ref_edge(&r, R::Resource, K::Texture, &x.texture_asset_id);
                for mask in &x.masks {
                    s.ref_edge(&r, R::Mask, K::Mesh, mask);
                }
            }
        }
        for id in m.offscreen_order() {
            if let Some(x) = m.get_offscreen(id) {
                let r = ObjectRef::new(K::Offscreen, id);
                s.add(r.clone(), &x.name, json!(x), Some(x.enabled));
                s.ref_edge(&r, R::PartOwner, K::Part, &x.part_id);
                for mask in &x.masks {
                    s.ref_edge(&r, R::Mask, K::Mesh, mask);
                }
            }
        }
        for id in m.parameter_order() {
            if let Some(x) = m.get_parameter(id) {
                s.add(ObjectRef::new(K::Parameter, id), &x.name, json!(x), None);
            }
        }
        for x in m.display_info().parameter_groups.iter().flatten() {
            let r = ObjectRef::new(K::ParameterGroup, &x.id);
            s.add(r.clone(), &x.name, json!(x), None);
            if let Some(parent) = &x.parent_id {
                s.ref_edge(&r, R::ParameterGroup, K::ParameterGroup, parent);
            }
        }
        for x in m.display_info().parameters.iter().flatten() {
            let r = match x {
                CdiParameterEntry::Resolved { parameter_id, .. } => {
                    ObjectRef::new(K::Parameter, parameter_id)
                }
                CdiParameterEntry::Unresolved { runtime_id, .. } => {
                    s.unresolved("CDI parameter", runtime_id)
                }
            };
            if let Some(group) = x.group_id() {
                s.ref_edge(&r, R::ParameterGroup, K::ParameterGroup, group);
            }
        }
        for id in m.binding_order() {
            if let Some(x) = m.get_binding(id) {
                s.add_binding(BindingInfo {
                    reference: ObjectRef::new(K::MeshBinding, id),
                    target: ObjectRef::new(K::Mesh, &x.mesh_id),
                    axes: x.axes.clone(),
                    points: x.keyforms.iter().map(|f| f.keys.clone()).collect(),
                    blend: false,
                });
            }
        }
        for id in m.scene_binding_order() {
            if let Some(x) = m.get_scene_binding(id) {
                let kind = match &x.track {
                    SceneTrack::Part { .. } => K::Part,
                    _ => K::Deformer,
                };
                s.add_binding(BindingInfo {
                    reference: ObjectRef::new(K::SceneBinding, id),
                    target: ObjectRef::new(kind, x.target_id()),
                    axes: x.axes.clone(),
                    points: x.track.samples().map(|f| f.keys.to_vec()).collect(),
                    blend: false,
                });
            }
        }
        for id in m.glue_order() {
            if let Some(x) = m.get_glue(id) {
                let r = ObjectRef::new(K::Glue, id);
                s.add(
                    r.clone(),
                    &x.name,
                    json!({"name":x.name,"intensity":x.intensity,"pairs":x.pairs,"runtime_id":x.runtime_id}),
                    None,
                );
                s.ref_edge(&r, R::GlueA, K::Mesh, &x.mesh_a_id);
                s.ref_edge(&r, R::GlueB, K::Mesh, &x.mesh_b_id);
                if let Some(b) = &x.binding {
                    s.add_binding(BindingInfo {
                        reference: ObjectRef::new(K::GlueBinding, id),
                        target: r,
                        axes: b.axes.clone(),
                        points: grid_points(&b.axes),
                        blend: false,
                    });
                }
            }
        }
        for id in m.blend_key_table_order() {
            if let Some(x) = m.get_blend_key_table(id) {
                let r = ObjectRef::new(K::BlendKeyTable, id);
                s.add(r.clone(), id, json!(x), None);
                s.edge(
                    &ObjectRef::new(K::Parameter, &x.parameter_id),
                    R::ParameterAxis,
                    r,
                    "shared BlendShape table",
                );
            }
        }
        for id in m.blend_constraint_order() {
            if let Some(x) = m.get_blend_constraint(id) {
                let r = ObjectRef::new(K::BlendConstraint, id);
                s.add(r.clone(), id, json!(x), None);
                s.edge(
                    &ObjectRef::new(K::Parameter, &x.parameter_id),
                    R::ParameterAxis,
                    r,
                    "constraint weights",
                );
            }
        }
        for id in m.blend_binding_order() {
            if let Some(x) = m.get_blend_binding(id) {
                let kind = match x.target_kind {
                    BlendShapeTargetKind::Mesh => K::Mesh,
                    BlendShapeTargetKind::Warp | BlendShapeTargetKind::Rotation => K::Deformer,
                    BlendShapeTargetKind::Part => K::Part,
                    BlendShapeTargetKind::Glue => K::Glue,
                    BlendShapeTargetKind::Offscreen => K::Offscreen,
                };
                let r = ObjectRef::new(K::BlendBinding, id);
                let axes = m
                    .get_blend_key_table(&x.key_table_id)
                    .map(|t| {
                        vec![BindingAxis {
                            parameter_id: t.parameter_id.clone(),
                            keys: t.keys.clone(),
                        }]
                    })
                    .unwrap_or_default();
                let points = grid_points(&axes);
                s.add_binding(BindingInfo {
                    reference: r.clone(),
                    target: ObjectRef::new(kind, &x.target_id),
                    axes,
                    points,
                    blend: true,
                });
                s.ref_edge(&r, R::KeyTable, K::BlendKeyTable, &x.key_table_id);
                for c in &x.constraint_ids {
                    s.ref_edge(&r, R::Constraint, K::BlendConstraint, c);
                }
            }
        }
        for (i, g) in m.motion_groups().iter().enumerate() {
            let r = ObjectRef::new(K::MotionGroup, i.to_string());
            s.add(r.clone(), &g.name, json!(g), None);
            for e in &g.entries {
                s.edge(
                    &r,
                    R::Contains,
                    ObjectRef::new(K::Motion, &e.clip_id),
                    format!("fade in {:?}, fade out {:?}", e.fade_in, e.fade_out),
                );
                if let Some(sound) = &e.sound {
                    s.ref_edge(&r, R::Resource, K::Attachment, sound);
                }
            }
        }
        for id in m.motion_order() {
            if let Some(x) = m.get_motion(id) {
                let r = ObjectRef::new(K::Motion, id);
                s.add(r.clone(),&x.name,json!({"name":x.name,"duration":x.duration,"fps":x.fps,"looping":x.looping,"fade_in":x.fade_in,"fade_out":x.fade_out,"events":x.events,"extensions":x.extensions}),None);
                for t in &x.tracks {
                    let tr = ObjectRef::new(K::MotionTrack, format!("{id}/{}", t.id));
                    s.add(tr.clone(),format!("Track {}",t.id),json!({"target":t.target,"initial":t.initial,"segments":t.segments.len(),"fade_in":t.fade_in,"fade_out":t.fade_out,"extensions":t.extensions}),None);
                    s.edge(&r, R::Contains, tr.clone(), "");
                    let target = match &t.target {
                        MotionTrackTarget::Parameter { parameter_id } => {
                            ObjectRef::new(K::Parameter, parameter_id)
                        }
                        MotionTrackTarget::PartOpacity { part_id } => {
                            ObjectRef::new(K::Part, part_id)
                        }
                        MotionTrackTarget::Model { .. } => doc.clone(),
                        MotionTrackTarget::Unresolved {
                            category,
                            runtime_id,
                        } => s.unresolved(category, runtime_id),
                    };
                    let detail = match &t.target {
                        MotionTrackTarget::Model { runtime_id } => {
                            format!("Model channel {runtime_id}")
                        }
                        _ => "".into(),
                    };
                    s.edge(&tr, R::MotionWrites, target, detail);
                }
            }
        }
        for id in m.expression_order() {
            if let Some(x) = m.get_expression(id) {
                let r = ObjectRef::new(K::Expression, id);
                s.add(r.clone(), &x.name, json!(x), None);
                for e in &x.entries {
                    let target = match &e.target {
                        ExpressionTarget::Resolved { parameter_id } => {
                            ObjectRef::new(K::Parameter, parameter_id)
                        }
                        ExpressionTarget::Unresolved { runtime_id } => {
                            s.unresolved("expression parameter", runtime_id)
                        }
                    };
                    s.edge(
                        &r,
                        R::ExpressionWrites,
                        target,
                        format!(
                            "{} · {:?}",
                            e.value,
                            e.blend
                                .unwrap_or(kasane_core::document::ExpressionBlend::Add)
                        ),
                    );
                }
            }
        }
        if let Some(x) = m.physics() {
            let r = ObjectRef::new(K::Physics, &x.id);
            s.add(
                r.clone(),
                "Physics",
                json!({"meta":x.data.meta,"extensions":x.data.extensions}),
                None,
            );
            for g in &x.data.settings {
                let gr = ObjectRef::new(K::PhysicsGroup, format!("{}/{}", x.id, g.id));
                let name = x
                    .data
                    .meta
                    .dictionary
                    .iter()
                    .find(|d| d.id == g.id)
                    .map_or(g.id.as_str(), |d| d.name.as_str());
                let mut payload = json!(g);
                payload["name"] = json!(name);
                payload["fps"] = json!(x.data.meta.fps.unwrap_or(30.));
                s.add(gr.clone(), name, payload, None);
                s.edge(&r, R::Contains, gr.clone(), "");
                for input in &g.inputs {
                    let target = if input.source.target == "Parameter" {
                        x.parameter_bindings
                            .get(&input.source.id)
                            .map(|id| ObjectRef::new(K::Parameter, id))
                    } else {
                        None
                    }
                    .unwrap_or_else(|| s.unresolved("physics input", &input.source.id));
                    s.edge(
                        &gr,
                        R::PhysicsInput,
                        target,
                        format!(
                            "{:?} · weight {} · reflect {}; saved configuration",
                            input.kind, input.weight, input.reflect
                        ),
                    );
                }
                for output in &g.outputs {
                    let target = if output.destination.target == "Parameter" {
                        x.parameter_bindings
                            .get(&output.destination.id)
                            .map(|id| ObjectRef::new(K::Parameter, id))
                    } else {
                        None
                    }
                    .unwrap_or_else(|| s.unresolved("physics output", &output.destination.id));
                    s.edge(
                        &gr,
                        R::PhysicsOutput,
                        target,
                        format!(
                            "{:?} · vertex {} · weight {} · scale {}; saved configuration",
                            output.kind, output.vertex_index, output.weight, output.scale
                        ),
                    );
                }
            }
        }
        if let Some(x) = m.pose() {
            let r = ObjectRef::new(K::Pose, &x.id);
            s.add(
                r.clone(),
                "Pose",
                json!({"fade_in":x.fade_in,"file_type":x.file_type,"extensions":x.extensions}),
                None,
            );
            for (i, g) in x.groups.iter().enumerate() {
                let gr = ObjectRef::new(K::PoseGroup, format!("{}/{i}", x.id));
                s.add(gr.clone(), format!("Group {}", i + 1), Value::Null, None);
                s.edge(&r, R::Contains, gr.clone(), "");
                for (j, e) in g.iter().enumerate() {
                    let er = ObjectRef::new(K::PoseEntry, format!("{}/{i}/{j}", x.id));
                    s.add(er.clone(), format!("Member {}", j + 1), json!(e), None);
                    s.edge(&gr, R::Contains, er.clone(), "");
                    let target = s.pose_ref(&e.part);
                    s.edge(&er, R::PoseMember, target, "saved visibility switch");
                    for link in e.links.iter().flatten() {
                        let target = s.pose_ref(link);
                        s.edge(&er, R::PoseLink, target, "");
                    }
                }
            }
        }
        for id in m.asset_order() {
            if let Some(x) = m.get_asset(id) {
                s.add(ObjectRef::new(K::Texture, id), &x.name, json!(x), None);
            }
        }
        for x in m.package_attachments() {
            s.add(
                ObjectRef::new(K::Attachment, &x.path),
                &x.path,
                json!({"path":x.path,"bytes":x.bytes.len()}),
                None,
            );
        }
        if let Some(path) = &m.model3_settings().user_data {
            s.ref_edge(&doc, R::Resource, K::Attachment, path);
        }
        for area in m.model3_settings().hit_areas.iter().flatten() {
            let target = match &area.mesh {
                ModelTargetRef::Resolved { object_id } => ObjectRef::new(K::Mesh, object_id),
                ModelTargetRef::Unresolved { runtime_id } => s.unresolved("hit area", runtime_id),
            };
            s.edge(&doc, R::HitArea, target, &area.name);
        }
        for group in m.model3_settings().groups.iter().flatten() {
            for p in &group.parameters {
                let target = match p {
                    ModelTargetRef::Resolved { object_id } => {
                        ObjectRef::new(K::Parameter, object_id)
                    }
                    ModelTargetRef::Unresolved { runtime_id } => {
                        s.unresolved("model parameter group", runtime_id)
                    }
                };
                s.edge(&doc, R::Contains, target, &group.name);
            }
        }
        // Missing endpoints stay navigable diagnostic objects, distinct from unused objects.
        let missing: Vec<_> = s
            .relations
            .iter()
            .flat_map(|e| [e.source.clone(), e.target.clone()])
            .filter(|r| s.get(r).is_none())
            .collect();
        for r in missing {
            if s.get(&r).is_none() {
                s.add(r.clone(), &r.id, Value::Null, None);
                s.objects.last_mut().unwrap().diagnostic = Some("Missing reference".into());
            }
        }
        for path in m.missing_attachments() {
            let r = ObjectRef::new(K::Attachment, path);
            if s.get(&r).is_none() {
                s.add(r, path, Value::Null, None);
                s.objects.last_mut().unwrap().diagnostic = Some("Missing attachment".into());
            }
        }
        s.build_influences();
        s
    }
    fn pose_ref(&mut self, p: &PosePartRef) -> ObjectRef {
        match p {
            PosePartRef::Resolved { part_id } => ObjectRef::new(ObjectKind::Part, part_id),
            PosePartRef::Unresolved { runtime_id } => self.unresolved("pose Part", runtime_id),
        }
    }
}
