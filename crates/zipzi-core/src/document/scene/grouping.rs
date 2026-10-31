//! Insert a neutral parent without changing the selected roots' evaluated shape.
use super::*;
use crate::{DeltaKeyforms, PreciseVec2, SceneTrack, TransformData, WarpBezier};

#[derive(Clone, Copy)]
struct Coordinates([f64; 4]);
impl Coordinates {
    fn point(self, p: Vec2) -> Vec2 {
        let [x, y, w, h] = self.0;
        Vec2::new(((p.x as f64 - x) / w) as f32, ((p.y as f64 - y) / h) as f32)
    }
    fn delta(self, p: Vec2) -> Vec2 {
        Vec2::new(
            (p.x as f64 / self.0[2]) as f32,
            (p.y as f64 / self.0[3]) as f32,
        )
    }
    fn origin(self, p: &mut PreciseVec2) {
        p.x = (p.x - self.0[0]) / self.0[2];
        p.y = (p.y - self.0[1]) / self.0[3];
    }
    fn points(self, points: &mut [Vec2]) {
        for p in points {
            *p = self.point(*p);
        }
    }
    fn bezier(self, bezier: &mut Option<WarpBezier>) {
        if let Some(bezier) = bezier {
            for node in &mut bezier.nodes {
                node.position = self.point(node.position);
                self.points(&mut node.handles);
            }
        }
    }
}

impl Document {
    /// Creation and selection parenting share one native, snapshot-bound plan.
    /// An existing Warp is passed through at its exact lattice resolution.
    pub fn plan_deformer_group(
        &self,
        create: SceneEdit,
        sources: Vec<String>,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<ScenePlan, Status> {
        control.checkpoint()?;
        limits.check_conversion(
            sources.len(),
            self.estimated_content_bytes().saturating_mul(4),
            self.estimated_content_bytes(),
        )?;
        let SceneEdit::Create {
            id,
            name,
            object,
            part_id,
            before,
            ..
        } = create
        else {
            return Err(Status::error(
                "EXPECTED_DEFORMER_CREATION",
                "Create a Warp or Rotation",
            ));
        };
        let warp = matches!(object, SceneCreate::Warp { .. });
        if !warp && !matches!(object, SceneCreate::Rotation) {
            return Err(Status::error("EXPECTED_DEFORMER_CREATION", &id));
        }
        let selected: HashSet<_> = sources.iter().cloned().collect();
        if selected.is_empty() || selected.len() != sources.len() {
            return Err(Status::error(
                "INVALID_DEFORMER_SELECTION",
                "Select each source once",
            ));
        }
        for source in &sources {
            if self.scene_deformer_parent(source).is_none() {
                return Err(Status::error("EXPECTED_DEFORMATION_MEMBERS", source));
            }
        }
        let mut seen = HashSet::new();
        let roots: Vec<_> = self
            .hierarchy_order()
            .deformation
            .iter()
            .chain(self.transform_order())
            .chain(self.mesh_order())
            .filter(|source| selected.contains(*source) && seen.insert((*source).clone()))
            .filter(|source| {
                let mut parent = self.scene_deformer_parent(source).unwrap_or("");
                while !parent.is_empty() {
                    if selected.contains(parent) {
                        return false;
                    }
                    parent = self.scene_deformer_parent(parent).unwrap_or("");
                }
                true
            })
            .cloned()
            .collect();
        let parent = self
            .scene_deformer_parent(&roots[0])
            .unwrap_or("")
            .to_owned();
        if roots
            .iter()
            .any(|source| self.scene_deformer_parent(source) != Some(parent.as_str()))
        {
            return Err(Status::error(
                "DEFORMER_GROUP_DIFFERENT_PARENTS",
                "Selected roots must share a deformation parent",
            ));
        }
        // Rotation inheritance carries direction and scale, but not reflection.
        // Check every stored pose, including inactive keyforms: a neutral child
        // cannot preserve a reflected parent's mapping over its parameter domain.
        if !warp
            && self.get_transform(&parent).is_some_and(|t| {
                t.rotation()
                    .is_some_and(|r| r.pose.reflect_x || r.pose.reflect_y)
                    || self.binding_for_scene(&parent).is_some_and(|b| {
                        matches!(&b.track, SceneTrack::Rotation { keyforms, .. }
                        if keyforms.iter().any(|f| f.rotation.reflect_x || f.rotation.reflect_y))
                    })
            })
        {
            return Err(Status::error(
                "ROTATION_GROUP_REFLECTION_UNPROVEN",
                "A Rotation group cannot preserve a reflected Rotation parent",
            ));
        }
        let old_warp = self.get_transform(&parent).and_then(Transform::warp);
        let rotation_coordinates = if !warp && old_warp.is_some() {
            Some(self.rotation_group_coordinates(&parent).ok_or_else(|| {
                Status::error(
                    "ROTATION_GROUP_UNDER_WARP_UNPROVEN",
                    "A Rotation cannot preserve this Warp coordinate space",
                )
            })?)
        } else {
            None
        };
        let mut bounds: Option<[f64; 4]> = None;
        let mut add = |p: Vec2| {
            let (x, y) = (p.x as f64, p.y as f64);
            bounds = Some(bounds.map_or([x, y, x, y], |[a, b, c, d]| {
                [a.min(x), b.min(y), c.max(x), d.max(y)]
            }));
        };
        for source in &roots {
            control.checkpoint()?;
            if let Some(mesh) = self.get_mesh(source) {
                for p in &mesh.base_positions {
                    add(*p);
                }
                if let Some(binding) = self.binding_for_mesh(source) {
                    for form in &binding.keyforms {
                        for p in &form.positions {
                            add(*p);
                        }
                    }
                }
            } else if let Some(transform) = self.get_transform(source) {
                if let Some(w) = transform.warp() {
                    for p in &w.points {
                        add(*p);
                    }
                } else {
                    let canvas = self.source_canvas();
                    add(Vec2::default());
                    add(Vec2::new(
                        canvas.map_or(100., |c| c.width),
                        canvas.map_or(100., |c| c.height),
                    ));
                }
            }
        }
        let bounds = bounds.unwrap_or([0., 0., 1., 1.]);
        let extent = (bounds[2] - bounds[0]).max(bounds[3] - bounds[1]).max(1.);
        let padding = extent * 0.05;
        let bounds = [
            bounds[0] - padding,
            bounds[1] - padding,
            bounds[2] + padding,
            bounds[3] + padding,
        ];
        let coordinates = (warp && old_warp.is_none())
            .then_some(Coordinates([
                bounds[0],
                bounds[1],
                bounds[2] - bounds[0],
                bounds[3] - bounds[1],
            ]))
            .or(rotation_coordinates);
        let parent_id = (!parent.is_empty()).then_some(parent.clone());
        let create = self.plan_scene_edit_with_control(
            SceneEdit::Create {
                id: id.clone(),
                name,
                object: if warp {
                    SceneCreate::Warp {
                        bounds: Some(if old_warp.is_some() {
                            [0., 0., 1., 1.]
                        } else {
                            bounds.map(|v| v as f32)
                        }),
                    }
                } else {
                    SceneCreate::Rotation
                },
                part_id,
                parent_id,
                before,
                deformation_before: Some(roots[0].clone()),
            },
            limits,
            control,
        )?;
        let mut candidate = self.clone();
        let result = candidate.apply_scene_plan(&create);
        if !result.status.is_ok() {
            return Err(result.status);
        }
        if let Some(old) = old_warp.filter(|_| warp) {
            let mut transform = candidate.get_transform(&id).unwrap().clone();
            let w = transform.warp_mut().unwrap();
            w.rows = old.rows;
            w.columns = old.columns;
            w.quad = old.quad;
            w.points = (0..=old.rows)
                .flat_map(|row| {
                    (0..=old.columns).map(move |column| {
                        Vec2::new(
                            column as f32 / old.columns as f32,
                            row as f32 / old.rows as f32,
                        )
                    })
                })
                .collect();
            let status = candidate.replace_transform(transform);
            if !status.status.is_ok() {
                return Err(status.status);
            }
        }
        let reparent = candidate.plan_scene_edit_with_control(
            SceneEdit::Reparent {
                edit: ReparentEdit::keep_local(sources, id.clone()),
            },
            limits,
            control,
        )?;
        let result = candidate.apply_scene_plan(&reparent);
        if !result.status.is_ok() {
            return Err(result.status);
        }
        // The final pass-through lattice can differ from the provisional
        // creation bounds. Capture the actual inserted deformer at the frozen
        // authoring inputs, after that normalization.
        candidate
            .content
            .transforms
            .get_mut(&id)
            .unwrap()
            .original_shape = None;
        candidate.initialize_deformer_original(&id);
        let mut plan = self.empty_scene_plan();
        plan.domain
            .affected
            .extend(reparent.affected_ids().iter().cloned());
        plan.domain
            .delta
            .transforms
            .push(candidate.get_transform(&id).unwrap().clone());
        plan.domain.affected.push(id.clone());
        for source in &roots {
            control.checkpoint()?;
            if let Some(mesh) = candidate.get_mesh(source) {
                let mut mesh = mesh.clone();
                if let Some(c) = coordinates {
                    c.points(&mut mesh.base_positions);
                }
                plan.domain.delta.meshes.push(mesh);
                if let Some(binding) = self.binding_for_mesh(source) {
                    let mut binding = binding.clone();
                    if let Some(c) = coordinates {
                        for form in &mut binding.keyforms {
                            c.points(&mut form.positions);
                        }
                    }
                    if self.get_binding(&binding.id) != Some(&binding) {
                        plan.domain.affected.push(binding.id.clone());
                        plan.domain.delta.bindings.push(binding);
                    }
                }
            } else {
                let mut transform = candidate.get_transform(source).unwrap().clone();
                if let Some(c) = coordinates {
                    match &mut transform.data {
                        TransformData::Warp(w) => {
                            c.points(&mut w.points);
                            c.bezier(&mut w.bezier);
                        }
                        TransformData::Rotation(r) => c.origin(&mut r.pose.origin),
                    }
                }
                plan.domain.delta.transforms.push(transform);
                if let Some(binding) = self.binding_for_scene(source) {
                    let mut binding = binding.clone();
                    if let Some(c) = coordinates {
                        match &mut binding.track {
                            SceneTrack::Warp { keyforms, .. } => {
                                for form in keyforms {
                                    c.points(&mut form.positions);
                                    c.bezier(&mut form.bezier);
                                }
                            }
                            SceneTrack::Rotation { keyforms, .. } => {
                                for form in keyforms {
                                    c.origin(&mut form.rotation.origin);
                                }
                            }
                            _ => unreachable!(),
                        }
                    }
                    if self.get_scene_binding(&binding.id) != Some(&binding) {
                        plan.domain.affected.push(binding.id.clone());
                        plan.domain.delta.scene_bindings.push(binding);
                    }
                }
            }
            if let Some(c) = coordinates {
                for binding in self.blend_bindings_for_target(source) {
                    let mut binding = binding.clone();
                    match &mut binding.keyforms {
                        DeltaKeyforms::Mesh(forms) => {
                            for form in forms {
                                for p in &mut form.positions {
                                    *p = c.delta(*p);
                                }
                            }
                        }
                        DeltaKeyforms::Warp(forms) => {
                            for form in forms {
                                for p in &mut form.points {
                                    *p = c.delta(*p);
                                }
                            }
                        }
                        DeltaKeyforms::Rotation(forms) => {
                            for form in forms {
                                if let Some(p) = &mut form.origin {
                                    *p = c.delta(*p);
                                }
                            }
                        }
                        _ => return Err(Status::error("WRONG_SHAPE_TARGET", &binding.id)),
                    }
                    if self.get_blend_binding(&binding.id) != Some(&binding) {
                        plan.domain.affected.push(binding.id.clone());
                        plan.domain.delta.blends.push(binding);
                    }
                }
            }
            plan.domain.affected.push(source.clone());
        }
        plan.domain.delta.hierarchy = Some(candidate.hierarchy_order().clone());
        plan.domain.affected.sort();
        plan.domain.affected.dedup();
        plan.domain.no_op = false;
        plan.cost = Some(limits.check_conversion(
            plan.domain.affected.len(),
            candidate.estimated_content_bytes().saturating_mul(2),
            self.estimated_content_bytes(),
        )?);
        let mut validation = self.clone();
        let result = validation.apply_scene_plan(&plan);
        if !result.status.is_ok() {
            return Err(result.status);
        }
        // A native-frame check catches mistakes in the coordinate conversion.
        // This supplements the structural domain checks above; checking one pose
        // alone cannot establish preservation of animated channels.
        let frame_bytes = self.mesh_order().iter().fold(0usize, |bytes, id| {
            bytes.saturating_add(
                self.get_mesh(id)
                    .unwrap()
                    .base_positions
                    .len()
                    .saturating_mul(8 * std::mem::size_of::<Vec2>()),
            )
        });
        plan.cost = Some(
            limits.check_conversion(
                plan.domain.affected.len(),
                candidate
                    .estimated_content_bytes()
                    .saturating_mul(2)
                    .saturating_add(frame_bytes),
                self.estimated_content_bytes(),
            )?,
        );
        control.checkpoint()?;
        let mut before = crate::DrawableFrame::default();
        let mut after = crate::DrawableFrame::default();
        let values = Default::default();
        for (document, frame) in [(self, &mut before), (&validation, &mut after)] {
            control.checkpoint()?;
            let status =
                crate::evaluation::evaluate_frame_including_hidden(document, &values, frame);
            if !status.is_ok() {
                return Err(status);
            }
        }
        let after: std::collections::HashMap<_, _> =
            after.drawables.iter().map(|d| (d.id.as_str(), d)).collect();
        for before in &before.drawables {
            control.checkpoint()?;
            let preserved = after.get(before.id.as_str()).is_some_and(|after| {
                before.positions.len() == after.positions.len()
                    && before.positions.iter().zip(&after.positions).all(|(a, b)| {
                        let error = (f64::from(a.x) - f64::from(b.x))
                            .hypot(f64::from(a.y) - f64::from(b.y));
                        let scale = f64::from(a.x).hypot(f64::from(a.y)).max(1.);
                        error.is_finite() && error <= 0.01 + 1e-6 * scale
                    })
            });
            if !preserved {
                return Err(Status::error("REPARENT_TOLERANCE_EXCEEDED", &before.id));
            }
        }
        Ok(plan)
    }

    fn rotation_group_coordinates(&self, parent: &str) -> Option<Coordinates> {
        if self.binding_for_scene(parent).is_some()
            || !self.blend_bindings_for_target(parent).is_empty()
        {
            return None;
        }
        let transform = self.get_transform(parent)?;
        let warp = transform.warp()?;
        if warp.bezier.is_some() {
            return None;
        }
        let origin = warp.points[0];
        let x = warp.points[warp.columns as usize].x - origin.x;
        let y = warp.points[(warp.rows * (warp.columns + 1)) as usize].y - origin.y;
        if x <= 0. || y <= 0. {
            return None;
        }
        let tolerance = x.max(y).max(1.) * 1e-5;
        for row in 0..=warp.rows {
            for column in 0..=warp.columns {
                let p = warp.points[(row * (warp.columns + 1) + column) as usize];
                if (p.x - origin.x - x * column as f32 / warp.columns as f32).abs() > tolerance
                    || (p.y - origin.y - y * row as f32 / warp.rows as f32).abs() > tolerance
                {
                    return None;
                }
            }
        }
        let mut ancestor = transform.parent();
        while !ancestor.is_empty() {
            let transform = self.get_transform(ancestor)?;
            let rotation = transform.rotation()?;
            if rotation.pose.reflect_x || rotation.pose.reflect_y {
                return None;
            }
            if let Some(binding) = self.binding_for_scene(ancestor) {
                if let SceneTrack::Rotation { keyforms, .. } = &binding.track {
                    if keyforms
                        .iter()
                        .any(|form| form.rotation.reflect_x || form.rotation.reflect_y)
                    {
                        return None;
                    }
                }
            }
            ancestor = transform.parent();
        }
        // Warp parents map origins but do not contribute their lattice scale to
        // Rotation inheritance. Move that fixed scale into each child's inputs.
        Some(Coordinates([0., 0., 1. / x as f64, 1. / y as f64]))
    }
}
