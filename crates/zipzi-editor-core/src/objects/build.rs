//! Build the immutable panel projection from the authoring document.
use super::bindings::grid_points;
use super::properties::deformer_properties;
use super::{BindingInfo, ObjectIndex, ObjectInfo, ObjectKind, ObjectRef, Relation, RelationKind};
use serde_json::{Value, json};
use zipzi_core::SceneTrackEvaluation;
use zipzi_core::document::PhysicsTargetProjection;
use zipzi_core::document::{
    CdiParameterEntry, ExpressionTarget, ModelTargetRef, MotionTrackTarget, PosePartRef,
};
use zipzi_core::{BindingAxis, BlendShapeTargetKind, Document as Model, SceneTrack};

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
        self.add(info.reference.clone(), format!("{} · {}", info.target.id, info.layout()), json!({"axes": info.axes, "keyforms": info.points.len(), "parameter_pad_available": info.supports_parameter_pad()}), None);
        for axis in info.axes.iter().filter(|a| !a.parameter_id.is_empty()) {
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
        let _benchmark_span = zipzi_core::benchmark::span("editor.object_index_build");
        use ObjectKind as K;
        use RelationKind as R;
        let mut s = Self {
            draw_groups: zipzi_core::draw_order::resolved_groups(m).into(),
            ..Default::default()
        };
        let doc = ObjectRef::new(K::Document, m.id());
        s.add(doc.clone(), "Document", json!({"export_settings":m.export_settings(),"source_canvas":m.source_canvas(),"model_settings":m.model3_settings(),"display_info":m.display_info(),"missing_attachments":m.missing_attachments(),"draw_groups":m.draw_order_groups()}), None);
        for group in s.draw_groups.clone().iter() {
            let role = ObjectRef::new(
                K::DrawOrderGroup,
                if group.owner.is_empty() {
                    m.id()
                } else {
                    &group.owner
                },
            );
            let name = if group.owner.is_empty() {
                "Drawing"
            } else {
                m.get_part(&group.owner)
                    .map_or(group.owner.as_str(), |p| p.name.as_str())
            };
            s.add(role.clone(), name, json!(group), None);
            for item in &group.items {
                s.ref_edge(
                    &role,
                    R::DrawMember,
                    if m.get_mesh(item).is_some() {
                        K::Mesh
                    } else {
                        K::Part
                    },
                    item,
                );
            }
        }
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
                s.add(r.clone(),&x.name,json!({"id":x.id,"name":x.name,"runtime_id":x.runtime_id,"part_id":x.part_id,"deformer_id":x.deformer_id,"texture_asset_id":x.texture_asset_id.as_str(),"texture_reference":x.texture_asset_id,"appearance":x.appearance,"draw_order":x.draw_order,"blend_mode":x.blend_mode,"enabled":x.enabled,"double_sided":x.double_sided,"inverted_mask":x.inverted_mask,"masks":x.masks,"vertices":x.vertex_ids.len(),"triangles":x.triangles.len()}),Some(x.enabled));
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
        for child in m.binding_identities_iter() {
            let kind = match child.kind {
                zipzi_core::document::BindingIdentityKind::Axis => K::BindingAxis,
                zipzi_core::document::BindingIdentityKind::Sample => K::BindingSample,
            };
            let value = if let Some(axis) = m
                .get_binding(&child.owner)
                .map(|b| b.axes.as_slice())
                .or_else(|| m.get_scene_binding(&child.owner).map(|b| b.axes.as_slice()))
                .or_else(|| {
                    m.get_glue(&child.owner)
                        .and_then(|g| g.binding.as_ref())
                        .map(|b| b.axes.as_slice())
                }) {
                axis.iter().find(|a| a.id == child.id).map_or_else(
                    || {
                        axis.iter()
                            .flat_map(|a| &a.samples)
                            .find(|s| s.id == child.id)
                            .map_or(json!({}), |s| json!({"value":s.value}))
                    },
                    |a| json!({"parameter_id":a.parameter_id,"samples":a.samples}),
                )
            } else if let Some(t) = m.get_blend_key_table(&child.owner) {
                t.samples.iter().find(|s| s.id == child.id).map_or(
                    json!({}),
                    |s| json!({"value":s.value,"base":t.base_sample_id==s.id}),
                )
            } else if let Some(c) = m.get_blend_constraint(&child.owner) {
                c.samples
                    .iter()
                    .find(|s| s.id == child.id)
                    .map_or(json!({}), |s| json!({"value":s.value,"weight":s.weight}))
            } else {
                json!({})
            };
            if kind == K::BindingAxis {
                if let Some(parameter) = value
                    .get("parameter_id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.is_empty())
                {
                    s.edge(
                        &ObjectRef::new(K::Parameter, parameter),
                        R::ParameterAxis,
                        ObjectRef::new(kind, &child.id),
                        "axis driver",
                    );
                }
            }
            s.add(
                ObjectRef::new(kind, &child.id),
                &child.id,
                json!({"owner":child.owner,"axis":child.axis,"fields":value}),
                None,
            );
            let owner_kind = if m.get_binding(&child.owner).is_some() {
                K::MeshBinding
            } else if m.get_scene_binding(&child.owner).is_some() {
                K::SceneBinding
            } else if m.get_glue(&child.owner).is_some() {
                K::Glue
            } else if m.get_blend_key_table(&child.owner).is_some() {
                K::BlendKeyTable
            } else {
                K::BlendConstraint
            };
            s.edge(
                &ObjectRef::new(kind, &child.id),
                R::EntryOf,
                ObjectRef::new(owner_kind, &child.owner),
                "binding-owned identity",
            );
        }
        for id in m.binding_order() {
            if let Some(x) = m.get_binding(id) {
                s.add_binding(BindingInfo {
                    reference: ObjectRef::new(K::MeshBinding, id),
                    target: ObjectRef::new(K::Mesh, &x.mesh_id),
                    axes: x.axes.clone().into(),
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
                    axes: x.axes.clone().into(),
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
                for (index, pair) in x.pairs.iter().enumerate() {
                    let child = ObjectRef::new(K::GlueVertexPair, &pair.id);
                    s.add(
                        child.clone(),
                        format!("Pair {}", index + 1),
                        json!(pair),
                        None,
                    );
                    s.edge(&r, R::Contains, child.clone(), "vertex pair");
                    if pair.vertex_a.is_none() || pair.vertex_b.is_none() {
                        s.objects.last_mut().unwrap().diagnostic =
                            Some("Choose both endpoint vertices".into());
                    }
                }
                s.ref_edge(&r, R::GlueA, K::Mesh, &x.mesh_a_id);
                s.ref_edge(&r, R::GlueB, K::Mesh, &x.mesh_b_id);
                if let Some(b) = &x.binding {
                    s.add_binding(BindingInfo {
                        reference: ObjectRef::new(K::GlueBinding, id),
                        target: r,
                        axes: b.axes.clone().into(),
                        points: grid_points(&b.axes).into(),
                        blend: false,
                    });
                }
            }
        }
        for id in m.blend_key_table_order() {
            if let Some(x) = m.get_blend_key_table(id) {
                let r = ObjectRef::new(K::BlendKeyTable, id);
                s.add(r.clone(), id, json!(x), None);
                if !x.parameter_id.is_empty() {
                    s.edge(
                        &ObjectRef::new(K::Parameter, &x.parameter_id),
                        R::ParameterAxis,
                        r,
                        "shared driver",
                    );
                }
            }
        }
        for id in m.blend_constraint_order() {
            if let Some(x) = m.get_blend_constraint(id) {
                let r = ObjectRef::new(K::BlendConstraint, id);
                s.add(r.clone(), id, json!(x), None);
                if !x.parameter_id.is_empty() {
                    s.edge(
                        &ObjectRef::new(K::Parameter, &x.parameter_id),
                        R::ParameterAxis,
                        r,
                        "shared driver",
                    );
                }
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
                            id: Default::default(),
                            samples: t.samples.clone(),
                            parameter_id: t.parameter_id.clone(),
                            keys: t.keys.clone(),
                        }]
                    })
                    .unwrap_or_default();
                let points = grid_points(&axes);
                s.add_binding(BindingInfo {
                    reference: r.clone(),
                    target: ObjectRef::new(kind, &x.target_id),
                    axes: axes.into(),
                    points: points.into(),
                    blend: true,
                });
                s.ref_edge(&r, R::KeyTable, K::BlendKeyTable, &x.key_table_id);
                for c in &x.constraint_ids {
                    s.ref_edge(&r, R::Constraint, K::BlendConstraint, c);
                }
            }
        }
        for g in m.motion_groups() {
            let r = ObjectRef::new(K::MotionGroup, &g.id);
            s.add(r.clone(), &g.name, json!(g), None);
            for e in &g.entries {
                s.edge(
                    &r,
                    R::Contains,
                    ObjectRef::new(K::MotionRegistration, &e.id),
                    "",
                );
            }
        }
        for e in m.motion_registrations() {
            let r = ObjectRef::new(K::MotionRegistration, &e.id);
            s.add(
                r.clone(),
                m.get_motion(&e.clip_id)
                    .map_or("Missing motion", |c| c.name.as_str()),
                json!(e),
                None,
            );
            s.ref_edge(&r, R::Resource, K::Motion, &e.clip_id);
            if let Some(reference) = &e.sound {
                let target = match reference {
                    zipzi_core::document::AttachmentRef::Resolved { attachment_id } => {
                        ObjectRef::new(K::Attachment, attachment_id)
                    }
                    zipzi_core::document::AttachmentRef::Unresolved { path } => {
                        s.unresolved("sound", path)
                    }
                };
                s.edge(&r, R::Resource, target, "");
            }
        }
        for id in m.motion_order() {
            if let Some(x) = m.get_motion(id) {
                let r = ObjectRef::new(K::Motion, id);
                s.add(
                    r.clone(),
                    &x.name,
                    super::motions::motion_properties(x),
                    None,
                );
                for t in &x.tracks {
                    let tr = ObjectRef::new(K::MotionTrack, &t.id);
                    s.add(tr.clone(),format!("Track {}",t.id),json!({"target":t.target,"initial":t.initial,"segments":t.segments.len(),"fade_in":t.fade_in,"fade_out":t.fade_out,"extensions":t.extensions}),None);
                    s.edge(&r, R::Contains, tr.clone(), "");
                    for k in t.keys() {
                        let kr = ObjectRef::new(K::MotionKey, &k.id);
                        s.add(kr.clone(), format!("Key {}", k.time), json!(k), None);
                        s.edge(&tr, R::Contains, kr, "");
                    }
                    let target = match &t.target {
                        MotionTrackTarget::Unspecified => s.unresolved("NeedsConfiguration", &t.id),
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
        for id in m.motion_order() {
            for e in &m.get_motion(id).unwrap().events {
                let r = ObjectRef::new(K::MotionEvent, &e.id);
                s.add(r.clone(), &e.value, json!(e), None);
                s.edge(&ObjectRef::new(K::Motion, id), R::Contains, r, "");
            }
        }
        for id in m.expression_order() {
            if let Some(x) = m.get_expression(id) {
                let r = ObjectRef::new(K::Expression, id);
                s.add(r.clone(), &x.name, json!(x), None);
                for e in &x.entries {
                    let er = ObjectRef::new(K::ExpressionEntry, &e.id);
                    s.add(er.clone(), &e.id, json!(e), None);
                    let diagnostic = match &e.target {
                        ExpressionTarget::Unspecified => {
                            Some("Parameter needs configuration".into())
                        }
                        ExpressionTarget::Unresolved { runtime_id } => {
                            Some(format!("Unresolved parameter: {runtime_id}"))
                        }
                        _ => None,
                    };
                    s.objects.last_mut().unwrap().diagnostic = diagnostic;
                    s.edge(&er, R::EntryOf, r.clone(), "");
                    let target = match &e.target {
                        ExpressionTarget::Unspecified => continue,
                        ExpressionTarget::Resolved { parameter_id } => {
                            ObjectRef::new(K::Parameter, parameter_id)
                        }
                        ExpressionTarget::Unresolved { runtime_id } => {
                            s.unresolved("expression parameter", runtime_id)
                        }
                    };
                    s.edge(&er, R::ExpressionWrites, target.clone(), "");
                    s.edge(
                        &r,
                        R::ExpressionWrites,
                        target,
                        format!(
                            "{} · {:?}",
                            e.value,
                            e.blend
                                .unwrap_or(zipzi_core::document::ExpressionBlend::Add)
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
                let gr = ObjectRef::new(K::PhysicsGroup, &g.id);
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
                    let item = ObjectRef::new(K::PhysicsInput, &input.id);
                    s.add(
                        item.clone(),
                        format!("{:?} input", input.kind),
                        json!(input),
                        None,
                    );
                    s.objects.last_mut().unwrap().diagnostic =
                        input.source.disabled_reason().map(|r| format!("{r:?}"));
                    s.edge(&gr, R::Contains, item.clone(), "");
                    let target = input
                        .source
                        .parameter_id()
                        .map(|id| ObjectRef::new(K::Parameter, id))
                        .unwrap_or_else(|| {
                            s.unresolved("physics input", &format!("{:?}", input.source))
                        });
                    let detail = format!(
                        "{:?} · weight {} · reflect {}; saved configuration",
                        input.kind, input.weight, input.reflect
                    );
                    s.edge(&gr, R::PhysicsInput, target.clone(), detail.clone());
                    s.edge(&item, R::PhysicsInput, target, detail);
                }
                for output in &g.outputs {
                    let item = ObjectRef::new(K::PhysicsOutput, &output.id);
                    s.add(
                        item.clone(),
                        format!("{:?} output", output.kind),
                        json!(output),
                        None,
                    );
                    s.objects.last_mut().unwrap().diagnostic = output
                        .destination
                        .disabled_reason()
                        .map(|r| format!("{r:?}"));
                    s.edge(&gr, R::Contains, item.clone(), "");
                    let target = output
                        .destination
                        .parameter_id()
                        .map(|id| ObjectRef::new(K::Parameter, id))
                        .unwrap_or_else(|| {
                            s.unresolved("physics output", &format!("{:?}", output.destination))
                        });
                    let detail = format!(
                        "{:?} · node {} · weight {} · scale {}; saved configuration",
                        output.kind, output.node_id, output.weight, output.scale
                    );
                    s.edge(&gr, R::PhysicsOutput, target.clone(), detail.clone());
                    s.edge(&item, R::PhysicsOutput, target, detail);
                    s.edge(
                        &item,
                        R::PhysicsSample,
                        ObjectRef::new(K::PhysicsNode, &output.node_id),
                        "sample node",
                    );
                }
                for (i, node) in g.vertices.iter().enumerate() {
                    let item = ObjectRef::new(K::PhysicsNode, &node.id);
                    s.add(item.clone(), format!("Node {i}"), json!(node), None);
                    s.edge(&gr, R::Contains, item, "");
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
            for g in &x.groups {
                let gr = ObjectRef::new(K::PoseGroup, &g.id);
                s.add(gr.clone(), &g.name, json!({"name":g.name}), None);
                s.edge(&r, R::Contains, gr.clone(), "");
                for (j, e) in g.entries.iter().enumerate() {
                    let er = ObjectRef::new(K::PoseEntry, &e.id);
                    s.add(er.clone(), format!("Member {}", j + 1), json!(e), None);
                    s.edge(&gr, R::Contains, er.clone(), "");
                    if e.part != PosePartRef::Unspecified {
                        let target = s.pose_ref(&e.part);
                        s.edge(&er, R::PoseMember, target, "saved visibility switch");
                    } else {
                        s.objects.last_mut().unwrap().diagnostic =
                            Some("Choose a main Part".into());
                    }
                    for link in e.links.iter().flatten() {
                        let lr = ObjectRef::new(K::PoseLink, &link.id);
                        s.add(lr.clone(), "Link", json!(link), None);
                        s.edge(&er, R::Contains, lr.clone(), "");
                        if link.target != PosePartRef::Unspecified {
                            let target = s.pose_ref(&link.target);
                            s.edge(&lr, R::PoseLink, target, "");
                        } else {
                            s.objects.last_mut().unwrap().diagnostic =
                                Some("Choose a linked Part".into());
                        }
                    }
                }
            }
        }
        for id in m.asset_order() {
            if let Some(x) = m.get_asset(id) {
                s.add(ObjectRef::new(K::Texture, id), &x.name, json!(x), None);
                if x.content.is_none() {
                    s.objects.last_mut().unwrap().diagnostic = Some(match &x.unresolved_source {
                        Some(source) => format!("BrokenReference: image source {source}"),
                        None => "NeedsConfiguration: image content".into(),
                    });
                }
            }
        }
        for x in m.package_attachments() {
            s.add(
                ObjectRef::new(K::Attachment, &x.id),
                &x.path,
                json!({"path":x.path,"bytes":x.bytes.as_ref().map(|b|b.len())}),
                None,
            );
            if x.bytes.is_none() {
                s.objects.last_mut().unwrap().diagnostic =
                    Some("NeedsConfiguration: attachment content".into());
            }
        }
        if let Some(path) = &m.model3_settings().user_data {
            let target = match path {
                zipzi_core::document::AttachmentRef::Resolved { attachment_id } => {
                    ObjectRef::new(K::Attachment, attachment_id)
                }
                zipzi_core::document::AttachmentRef::Unresolved { path } => {
                    s.unresolved("UserData", path)
                }
            };
            s.edge(&doc, R::Resource, target, "");
        }
        for area in m.model3_settings().hit_areas.iter().flatten() {
            let r = ObjectRef::new(K::ModelHitArea, &area.id);
            s.add(r.clone(), &area.name, json!(area), None);
            s.edge(&doc, R::Contains, r.clone(), "HitArea");
            let target = match &area.mesh {
                ModelTargetRef::Resolved { object_id } => Some(ObjectRef::new(K::Mesh, object_id)),
                ModelTargetRef::Unresolved { runtime_id } => {
                    Some(s.unresolved("hit area", runtime_id))
                }
                ModelTargetRef::Unspecified => None,
            };
            if let Some(target) = target {
                s.edge(&r, R::HitArea, target, "");
            }
        }
        for group in m.model3_settings().groups.iter().flatten() {
            let r = ObjectRef::new(K::ModelParameterGroup, &group.id);
            s.add(r.clone(), &group.name, json!(group), None);
            s.edge(&doc, R::Contains, r.clone(), "behavior group");
            for (i, member) in group.parameters.iter().enumerate() {
                let child = ObjectRef::new(K::ModelParameterMember, &member.id);
                s.add(
                    child.clone(),
                    format!("Member {}", i + 1),
                    json!(member),
                    None,
                );
                s.edge(&r, R::Contains, child.clone(), "member");
                let target = match &member.target {
                    ModelTargetRef::Resolved { object_id } => {
                        Some(ObjectRef::new(K::Parameter, object_id))
                    }
                    ModelTargetRef::Unresolved { runtime_id } => {
                        Some(s.unresolved("model parameter group", runtime_id))
                    }
                    ModelTargetRef::Unspecified => None,
                };
                if let Some(target) = target {
                    s.edge(&child, R::ReferenceTarget, target, "");
                }
            }
        }
        for set in m.display_info().combined_parameters.iter().flatten() {
            let r = ObjectRef::new(K::CdiCombinedSet, &set.id);
            s.add(r.clone(), "Combined parameters", json!(set), None);
            s.edge(&doc, R::Contains, r.clone(), "combined set");
            for (i, member) in set.members.iter().enumerate() {
                let child = ObjectRef::new(K::CdiCombinedMember, &member.id);
                s.add(
                    child.clone(),
                    format!("Member {}", i + 1),
                    json!(member),
                    None,
                );
                s.edge(&r, R::Contains, child.clone(), "member");
                let target = match &member.target {
                    zipzi_core::document::CdiParameterRef::Resolved { parameter_id } => {
                        Some(ObjectRef::new(K::Parameter, parameter_id))
                    }
                    zipzi_core::document::CdiParameterRef::Unresolved { runtime_id } => {
                        Some(s.unresolved("combined set", runtime_id))
                    }
                    zipzi_core::document::CdiParameterRef::Unspecified => None,
                };
                if let Some(target) = target {
                    s.edge(&child, R::ReferenceTarget, target, "");
                }
            }
        }
        for disabled in &m.reference_projection().disabled {
            if let Some(identity) = m.reference_identity(&disabled.id) {
                let kind = match identity.kind {
                    zipzi_core::document::ReferenceKind::ModelParameterGroup => {
                        K::ModelParameterGroup
                    }
                    zipzi_core::document::ReferenceKind::ModelParameterMember => {
                        K::ModelParameterMember
                    }
                    zipzi_core::document::ReferenceKind::ModelHitArea => K::ModelHitArea,
                    zipzi_core::document::ReferenceKind::CdiCombinedSet => K::CdiCombinedSet,
                    zipzi_core::document::ReferenceKind::CdiCombinedMember => K::CdiCombinedMember,
                };
                if let Some(&slot) = s.structure.slots.get(&ObjectRef::new(kind, &disabled.id)) {
                    s.objects[slot].diagnostic =
                        Some(format!("NeedsConfiguration: {:?}", disabled.reason));
                }
            }
        }
        let availability = m.scene_projection();
        for slot in 0..s.objects.len() {
            let info = &mut s.objects[slot];
            if matches!(info.reference.kind, K::Mesh | K::Offscreen | K::Glue) {
                let diagnostics: Vec<_> = availability
                    .disabled
                    .iter()
                    .filter(|d| d.id == info.reference.id)
                    .collect();
                if !diagnostics.is_empty() {
                    info.diagnostic =
                        Some(format!("Needs configuration: {:?}", diagnostics[0].reason));
                }
                info.properties["availability"] = json!(diagnostics);
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
            PosePartRef::Unspecified => unreachable!("unconfigured references have no edge"),
            PosePartRef::Resolved { part_id } => ObjectRef::new(ObjectKind::Part, part_id),
            PosePartRef::Unresolved { runtime_id } => self.unresolved("pose Part", runtime_id),
        }
    }
}
