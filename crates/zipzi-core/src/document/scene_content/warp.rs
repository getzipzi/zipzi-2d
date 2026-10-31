use super::*;
use crate::{BlendShapeTargetKind, DeltaKeyforms, SceneTrack, WarpTransform};
use zipzi_core::WarpBezierEvaluation;
impl Document {
    pub(super) fn plan_warp_content(
        &self,
        plan: &mut SceneContentPlan,
        edit: SceneContentEdit,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        use SceneContentEdit as E;
        let owned_id = match &edit {
            E::WarpPoints { transform_id, .. }
            | E::WarpBezier { transform_id, .. }
            | E::WarpBezierControls { transform_id, .. }
            | E::WarpAuthoring { transform_id, .. }
            | E::WarpShape { transform_id, .. }
            | E::ResizeWarp { transform_id, .. } => transform_id.clone(),
            _ => unreachable!(),
        };
        let id = &owned_id;
        self.content_unlocked(id)?;
        let mut transform = self
            .get_transform(id)
            .ok_or_else(|| Status::error("MISSING_TRANSFORM", id))?
            .clone();
        let settings = transform.warp_authoring.clone().unwrap_or_default();
        if let E::WarpAuthoring { settings: next, .. } = &edit {
            if let Some(grid) = next
                .bezier_grid
                .filter(|grid| Some(*grid) != settings.bezier_grid)
            {
                check(grid.validate())?;
                let warp = transform
                    .warp_mut()
                    .ok_or_else(|| Status::error("NOT_WARP", id))?;
                if let Some(controller) = &mut warp.bezier {
                    *controller = controller.resized(grid);
                    plan.report.resampled_shapes += 1;
                }
                if let Some(binding) = self.binding_for_scene(id) {
                    let mut changed = binding.clone();
                    let SceneTrack::Warp { keyforms, .. } = &mut changed.track else {
                        return Err(Status::error("WRONG_SHAPE_TARGET", id));
                    };
                    for form in keyforms {
                        control.check()?;
                        if let Some(controller) = &mut form.bezier {
                            *controller = controller.resized(grid);
                            plan.report.resampled_shapes += 1;
                        }
                    }
                    if &changed != binding {
                        self.content_unlocked(&binding.id)?;
                        plan.domain.affected.push(binding.id.clone());
                        plan.domain.delta.scene_bindings.push(changed);
                    }
                }
            }
            if transform.warp().is_none() {
                return Err(Status::error("NOT_WARP", id));
            }
            transform.warp_authoring =
                (*next != crate::WarpAuthoringSettings::default()).then(|| next.clone());
            plan.domain.affected.push(id.clone());
            plan.domain.delta.transforms.push(transform);
            return Ok(());
        }
        let warp = transform
            .warp_mut()
            .ok_or_else(|| Status::error("NOT_WARP", id))?;
        plan.domain.affected.push(id.clone());
        match &edit {
            E::ResizeWarp {
                rows,
                columns,
                quad,
                ..
            } => {
                if *rows == 0 || *columns == 0 || *rows > 64 || *columns > 64 {
                    return Err(Status::error("INVALID_WARP_DIMENSIONS", id));
                }
                if (*rows, *columns, *quad) == (warp.rows, warp.columns, warp.quad) {
                    return Ok(());
                }
                let old = warp.clone();
                warp.points = resample(&old.points, &old, *rows, *columns, control)?;
                warp.rows = *rows;
                warp.columns = *columns;
                warp.quad = *quad;
                plan.report.resampled_shapes += 1;
                if let Some(b) = self.binding_for_scene(id) {
                    self.content_unlocked(&b.id)?;
                    let mut b = b.clone();
                    let SceneTrack::Warp { keyforms, .. } = &mut b.track else {
                        return Err(Status::error("WRONG_SHAPE_TARGET", id));
                    };
                    for f in keyforms {
                        f.positions = resample(&f.positions, &old, *rows, *columns, control)?;
                        plan.report.resampled_shapes += 1;
                    }
                    plan.domain.affected.push(b.id.clone());
                    plan.domain.delta.scene_bindings.push(b);
                }
                for b in self.iter_blend_bindings_for_target(id) {
                    self.content_unlocked(&b.id)?;
                    let mut b = b.clone();
                    let DeltaKeyforms::Warp(forms) = &mut b.keyforms else {
                        return Err(Status::error("WRONG_SHAPE_TARGET", id));
                    };
                    for f in forms {
                        f.points = resample(&f.points, &old, *rows, *columns, control)?;
                        plan.report.resampled_shapes += 1;
                    }
                    plan.domain.affected.push(b.id.clone());
                    plan.domain.delta.blends.push(b);
                }
                plan.report.losses.push("Warp resize samples each existing lattice at the new nodes; interpolation between nodes may change".into());
            }
            E::WarpPoints { target, .. }
            | E::WarpBezier { target, .. }
            | E::WarpBezierControls { target, .. }
            | E::WarpShape { target, .. } => match target {
                ShapeTarget::Base => edit_shape(
                    &mut warp.points,
                    &mut warp.bezier,
                    warp.rows,
                    warp.columns,
                    warp.quad,
                    &edit,
                    &settings,
                )?,
                ShapeTarget::Ordinary {
                    binding_id,
                    samples,
                } => {
                    self.content_unlocked(binding_id)?;
                    let mut b = self
                        .get_scene_binding(binding_id)
                        .filter(|b| b.target_id() == id)
                        .ok_or_else(|| Status::error("WRONG_SHAPE_TARGET", binding_id))?
                        .clone();
                    let SceneTrack::Warp { keyforms, .. } = &mut b.track else {
                        return Err(Status::error("WRONG_SHAPE_TARGET", binding_id));
                    };
                    let f = keyforms
                        .iter_mut()
                        .find(|f| super::mesh::same_samples(&f.samples, samples))
                        .ok_or_else(|| Status::error("MISSING_KEYFORM", binding_id))?;
                    edit_shape(
                        &mut f.positions,
                        &mut f.bezier,
                        warp.rows,
                        warp.columns,
                        warp.quad,
                        &edit,
                        &settings,
                    )?;
                    plan.domain.affected.push(b.id.clone());
                    plan.domain.delta.scene_bindings.push(b);
                }
                ShapeTarget::Blend {
                    binding_id,
                    sample_id,
                } => {
                    if !matches!(edit, E::WarpPoints { .. }) {
                        return Err(Status::error("BLEND_HAS_NO_BEZIER_CONTROLLER", binding_id));
                    }
                    self.content_unlocked(binding_id)?;
                    let mut b = self
                        .get_blend_binding(binding_id)
                        .filter(|b| {
                            b.target_id == *id && b.target_kind == BlendShapeTargetKind::Warp
                        })
                        .ok_or_else(|| Status::error("WRONG_SHAPE_TARGET", binding_id))?
                        .clone();
                    let DeltaKeyforms::Warp(forms) = &mut b.keyforms else {
                        unreachable!()
                    };
                    let f = forms
                        .iter_mut()
                        .find(|f| f.sample_id == *sample_id)
                        .ok_or_else(|| Status::error("MISSING_KEYFORM", sample_id))?;
                    edit_shape(
                        &mut f.points,
                        &mut None,
                        warp.rows,
                        warp.columns,
                        warp.quad,
                        &edit,
                        &settings,
                    )?;
                    plan.domain.affected.push(b.id.clone());
                    plan.domain.delta.blends.push(b);
                }
            },
            _ => unreachable!(),
        }
        plan.domain.delta.transforms.push(transform);
        Ok(())
    }
}
fn edit_shape(
    points: &mut Vec<Vec2>,
    bezier: &mut Option<WarpBezier>,
    rows: u32,
    columns: u32,
    quad: bool,
    edit: &SceneContentEdit,
    settings: &crate::WarpAuthoringSettings,
) -> Result<(), Status> {
    match edit {
        SceneContentEdit::WarpPoints {
            updates,
            clear_bezier,
            ..
        } => {
            distinct(
                &updates
                    .iter()
                    .map(|p| (p.row, p.column))
                    .collect::<Vec<_>>(),
            )?;
            for p in updates {
                if p.row > rows || p.column > columns {
                    return Err(Status::error("INVALID_WARP_POINT", "Point outside lattice"));
                }
                if !p.point.x.is_finite() || !p.point.y.is_finite() {
                    return Err(Status::error("INVALID_POSITION", "Non-finite Warp point"));
                }
                let slot = (p.row * (columns + 1) + p.column) as usize;
                points[slot] = p.point;
            }
            if *clear_bezier {
                *bezier = None;
            }
        }
        SceneContentEdit::WarpBezier { controller, .. } => {
            if let Some(next) = controller {
                check(next.validate())?;
                if settings
                    .bezier_grid
                    .is_some_and(|g| (g.rows, g.columns) != (next.rows, next.columns))
                {
                    return Err(Status::error(
                        "INVALID_WARP_BEZIER",
                        "Use the Warp controller density setting to change all stored shapes",
                    ));
                }
                let old = bezier
                    .clone()
                    .unwrap_or_else(|| derived_controller(rows, columns, quad, points, settings));
                // apply_delta preserves imported lattice residuals, including
                // controllers whose resolution differs from the runtime grid.
                if next != &old {
                    *points = if settings.auto_reform {
                        next.lattice(rows, columns)
                    } else {
                        next.apply_delta(&old, rows, columns, points)
                    };
                }
            }
            *bezier = controller.clone();
        }
        SceneContentEdit::WarpBezierControls { updates, .. } => {
            let before = bezier
                .clone()
                .unwrap_or_else(|| derived_controller(rows, columns, quad, points, settings));
            let next = before.edit_controls(updates, settings.edit_mode)?;
            if next == before {
                return Ok(());
            }
            *points = if settings.auto_reform {
                next.lattice(rows, columns)
            } else {
                next.apply_delta(&before, rows, columns, points)
            };
            *bezier = Some(next);
        }
        SceneContentEdit::WarpShape { action, .. } => match action {
            crate::WarpShapeAction::ResetController => {
                let grid = settings.bezier_grid.or_else(|| {
                    bezier.as_ref().map(|b| crate::WarpBezierGrid {
                        rows: b.rows,
                        columns: b.columns,
                    })
                });
                *bezier = Some(grid.map_or_else(
                    || WarpBezier::from_lattice(rows, columns, quad, points),
                    |grid| WarpBezier::from_lattice_grid(rows, columns, quad, points, grid),
                ));
            }
            crate::WarpShapeAction::Reform => {
                let controller = bezier
                    .clone()
                    .unwrap_or_else(|| derived_controller(rows, columns, quad, points, settings));
                *points = controller.lattice(rows, columns);
                *bezier = Some(controller);
            }
        },
        _ => unreachable!(),
    }
    Ok(())
}
fn derived_controller(
    rows: u32,
    columns: u32,
    quad: bool,
    points: &[Vec2],
    settings: &crate::WarpAuthoringSettings,
) -> WarpBezier {
    settings.bezier_grid.map_or_else(
        || WarpBezier::from_lattice(rows, columns, quad, points),
        |grid| WarpBezier::from_lattice_grid(rows, columns, quad, points, grid),
    )
}
fn resample(
    points: &[Vec2],
    old: &WarpTransform,
    rows: u32,
    columns: u32,
    control: &ConversionControl,
) -> Result<Vec<Vec2>, Status> {
    let flat: Vec<_> = points.iter().flat_map(|p| [p.x, p.y]).collect();
    let mut out = Vec::with_capacity(((rows + 1) * (columns + 1)) as usize);
    for row in 0..=rows {
        control.check()?;
        for column in 0..=columns {
            let mut p = [0.; 2];
            crate::deformers::warp_points(
                old.rows as i32,
                old.columns as i32,
                old.quad,
                &flat,
                &[column as f32 / columns as f32, row as f32 / rows as f32],
                &mut p,
                1,
            );
            out.push(Vec2::new(p[0], p[1]));
        }
    }
    Ok(out)
}
