use super::*;

impl Exporter<'_> {
    pub(super) fn parts(&mut self) -> Vec<N> {
        let mut sources = Vec::new();
        let mut root_runtime_id = String::from("__RootPart__");
        while self
            .doc
            .part_order()
            .iter()
            .any(|id| self.doc.get_part(id).unwrap().runtime_id == root_runtime_id)
        {
            root_runtime_id.push('_');
        }
        let root = Part {
            id: self.root_part.clone(),
            runtime_id: root_runtime_id,
            name: "Root Part".into(),
            draw_order: 500.,
            ..Default::default()
        };
        let mut parts = vec![root];
        parts.extend(
            self.doc
                .part_order()
                .iter()
                .map(|id| self.doc.get_part(id).unwrap().clone()),
        );
        for p in parts {
            let owner = self.source(&p.id);
            let binding = self.doc.binding_for_scene(&p.id);
            let samples: Vec<_> = binding
                .map(|v| {
                    v.track
                        .samples()
                        .map(|f| (f.keys.to_vec(), f.draw_order))
                        .collect()
                })
                .unwrap_or_else(|| vec![(Vec::new(), p.draw_order)]);
            let mut guids = Vec::new();
            let mut forms = Vec::new();
            let offscreen = self
                .doc
                .offscreen_order()
                .iter()
                .map(|id| self.doc.get_offscreen(id).unwrap())
                .find(|os| os.part_id == p.id);
            for (slot, (_, order)) in samples.iter().enumerate() {
                let (base, g) = self.form_base(&owner);
                guids.push(g);
                let a = offscreen
                    .and_then(|os| {
                        os.keyform_index(slot, binding.map(|b| b.track.len()))
                            .unwrap()
                            .map(|idx| &os.keyforms[idx])
                    })
                    .map_or(Appearance::default(), |form| Appearance {
                        opacity: form.opacity,
                        multiply: form.multiply.unwrap_or([1.; 3]),
                        screen: form.screen.unwrap_or([0.; 3]),
                    });
                let a = Appearance {
                    opacity: if offscreen.is_some_and(|os| !os.enabled) {
                        0.
                    } else {
                        a.opacity
                    },
                    ..a
                };
                forms.push(
                    self.graph.add(appearance(
                        N::new("CPartForm")
                            .field("super", base)
                            .field("drawOrder", i(order.round() as i32)),
                        a,
                    )),
                );
            }
            let grid = self.grid(
                binding.map_or(&[], |b| &b.axes),
                &guids,
                Some(samples.iter().map(|v| v.0.clone()).collect()),
            );
            let part = if p.id == self.root_part { "" } else { &p.id };
            let groups = kasane_core::draw_order::resolved_groups(self.doc);
            let mut group_owner = part;
            while !group_owner.is_empty() && !groups.iter().any(|g| g.owner == group_owner) {
                group_owner = &self.doc.get_part(group_owner).unwrap().parent_id;
            }
            let items = &groups
                .iter()
                .find(|g| g.owner == group_owner)
                .unwrap()
                .items;
            let mut children = ordered_drawing_children(self.doc, part, items);
            // Cubism's parts panel is front-to-back; Kasane group items break
            // equal draw-order ties back-to-front. Reverse at every tree level,
            // including ordinary parts flattened into an ancestor's group.
            children.reverse();
            children.extend(
                self.doc
                    .transform_order()
                    .iter()
                    .filter(|id| self.doc.get_transform(id).unwrap().part() == part)
                    .cloned(),
            );
            children.extend(
                self.doc
                    .glue_order()
                    .iter()
                    .filter(|id| {
                        self.doc
                            .get_mesh(&self.doc.get_glue(id).unwrap().mesh_a_id)
                            .unwrap()
                            .part_id
                            == part
                    })
                    .cloned(),
            );
            let source = N::new("CPartSource")
                .field(
                    "super",
                    self.controllable(
                        &p.name,
                        p.enabled,
                        if p.id == self.root_part {
                            &self.root_part
                        } else {
                            &p.parent_id
                        },
                        grid,
                        vec![],
                    ),
                )
                .field("guid", guid("CPartGuid", &p.id))
                .field("id", ident("CPartId", &p.runtime_id))
                .field("keyforms", N::list("carray_list", forms))
                .field(
                    "enableDrawOrderGroup",
                    b(groups.iter().any(|g| g.owner == part)),
                )
                .field("defaultOrder_forEditor", i(p.draw_order.round() as i32))
                .field("isSketch", b(false))
                .field("partsEditColor", N::new("CColor"))
                .field(
                    "_childGuids",
                    N::list(
                        "carray_list",
                        children.iter().map(|id| self.object_guid(id)),
                    ),
                )
                .field("useOffscreen", b(offscreen.is_some()))
                .field(
                    "clipGuidList",
                    N::list(
                        "carray_list",
                        offscreen
                            .into_iter()
                            .flat_map(|os| os.masks.iter())
                            .map(|id| guid("CDrawableGuid", id)),
                    ),
                )
                .field(
                    "invertClippingMask",
                    b(offscreen.is_some_and(|os| os.flags & 8 != 0)),
                )
                .field(
                    "colorComposition",
                    N::enumeration(
                        "ColorComposition",
                        color_mode(offscreen.map_or(0, |os| os.blend_mode)),
                    ),
                )
                .field(
                    "alphaComposition",
                    N::enumeration(
                        "AlphaComposition",
                        alpha_mode(offscreen.map_or(0, |os| os.blend_mode)),
                    ),
                );
            self.graph.set(&owner, source);
            sources.push(owner);
        }
        sources
    }
    pub(super) fn transforms(&mut self) -> Vec<N> {
        let mut sources = Vec::new();
        for id in self.doc.transform_order() {
            let t = self.doc.get_transform(id).unwrap();
            let owner = self.source(id);
            let binding = self.doc.binding_for_scene(id);
            let samples: Vec<_> =
                binding
                    .map(|v| v.track.samples().collect())
                    .unwrap_or_else(|| {
                        vec![SceneSample {
                            keys: &[],
                            positions: t.warp().map_or(&[], |w| &w.points),
                            rotation: t.rotation().map_or(RotationPose::default(), |r| r.pose),
                            appearance: t.appearance,
                            draw_order: 0.,
                        }]
                    });
            let mut forms = Vec::new();
            let mut guids = Vec::new();
            for sample in &samples {
                let (base, g) = self.form_base(&owner);
                guids.push(g);
                let base = appearance(
                    N::new("ACDeformerForm").field("super", base),
                    sample.appearance,
                )
                .field("coordType", coord(t.parent()));
                let form = if t.warp().is_some() {
                    N::new("CWarpDeformerForm").field("super", base).field(
                        "positions",
                        self.source_positions(t.parent(), sample.positions),
                    )
                } else {
                    // Cubism canvas roots are Y-down. Convert rotation orientation
                    // at serialization, including the parent direction probes.
                    let r = sample.rotation;
                    N::new("CRotationDeformerForm")
                        .attr("originX", self.source_origin_x(t.parent(), r.origin.x))
                        .attr("originY", self.source_origin_y(t.parent(), r.origin.y))
                        .attr("angle", self.rotation_sign() * r.angle)
                        .attr("scale", r.scale)
                        .attr("isReflectX", r.reflect_x)
                        .attr("isReflectY", !r.reflect_y)
                        .field("super", base)
                };
                forms.push(self.graph.add(form));
            }
            let grid = self.grid(
                binding.map_or(&[], |b| &b.axes),
                &guids,
                Some(samples.iter().map(|f| f.keys.to_vec()).collect()),
            );
            let base = N::new("ACDeformerSource")
                .field(
                    "super",
                    self.controllable(&t.name, t.enabled, t.part(), grid, vec![]),
                )
                .field("guid", guid("CDeformerGuid", id))
                .field("id", ident("CDeformerId", &t.runtime_id))
                .field("targetDeformerGuid", self.parent_deformer(t.parent()));
            let source = match &t.data {
                TransformData::Warp(w) => N::new("CWarpDeformerSource")
                    .field("super", base)
                    .field("col", i(w.columns))
                    .field("row", i(w.rows))
                    .field("isQuadTransform", b(w.quad))
                    .field("keyforms", N::list("carray_list", forms)),
                TransformData::Rotation(r) => N::new("CRotationDeformerSource")
                    .field("super", base)
                    .field("useBoneUi_testImpl", b(false))
                    .field("keyforms", N::list("carray_list", forms))
                    .field("handleLengthOnCanvas", f(100))
                    .field("circleRadiusOnCanvas", f(20))
                    .field(
                        "baseAngle",
                        f(self.rotation_sign() * r.base_angle
                            - if !t.parent().is_empty() { 180. } else { 0. }),
                    ),
            };
            self.graph.set(&owner, source);
            sources.push(owner);
        }
        sources
    }
}
