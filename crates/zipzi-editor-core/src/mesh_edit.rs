//! Invert the displayed pose without changing unselected authored vertices.
use crate::{ModelPoint, ModelVector};
use anyhow::{Result, ensure};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub(crate) struct PreviewConstraint(pub String);
impl std::fmt::Display for PreviewConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for PreviewConstraint {}

pub(crate) type Vertex = (String, u32);
const MIN_GAIN: f64 = 0.05;

#[derive(Clone)]
pub(crate) struct MeshEdit {
    pub local: Vec<zipzi_core::Vec2>,
    pub displayed: Vec<ModelPoint>,
    pub weight: f32,
    pub glue: std::sync::Arc<GlueMap>,
    pub original_points: bool,
}

#[derive(Default, Debug)]
pub(crate) struct GlueMap {
    rows: HashMap<Vertex, HashMap<Vertex, f64>>,
}
impl GlueMap {
    pub fn new(operations: &[zipzi_core::evaluation::GlueOperation]) -> Self {
        let mut rows = HashMap::<Vertex, HashMap<Vertex, f64>>::new();
        for op in operations {
            let a = (op.mesh_a.clone(), op.vertex_a);
            let b = (op.mesh_b.clone(), op.vertex_b);
            if a == b {
                continue;
            }
            let ra = rows
                .entry(a.clone())
                .or_insert_with(|| HashMap::from([(a.clone(), 1.)]))
                .clone();
            let rb = rows
                .entry(b.clone())
                .or_insert_with(|| HashMap::from([(b.clone(), 1.)]))
                .clone();
            let mix = |amount: f64| {
                let mut row = HashMap::new();
                for (k, v) in &ra {
                    *row.entry(k.clone()).or_default() += v * (1. - amount);
                }
                for (k, v) in &rb {
                    *row.entry(k.clone()).or_default() += v * amount;
                }
                row.retain(|_, v: &mut f64| v.abs() > 1e-14);
                row
            };
            rows.insert(a, mix(f64::from(op.amount_a)));
            rows.insert(b, mix(1. - f64::from(op.amount_b)));
        }
        Self { rows }
    }

    pub(crate) fn affected_vertices(&self) -> impl Iterator<Item = &Vertex> {
        self.rows.keys()
    }

    /// Probe the same translation and amplification limits as the actual drag.
    pub(crate) fn check_drag(
        &self,
        vertices: &[(Vertex, f32)],
        original_points: bool,
    ) -> Result<()> {
        ensure!(
            vertices
                .iter()
                .all(|(_, w)| w.is_finite() && *w >= MIN_GAIN as f32),
            "BlendShape influence is too weak; adjust its constraints before editing"
        );
        if original_points {
            return Ok(());
        }
        // Unaffected vertices share their object's decision; only Glue sources
        // need a joint inverse. This keeps large marquee selections cheap.
        let affected: Vec<_> = vertices
            .iter()
            .filter(|(v, _)| self.rows.contains_key(v))
            .collect();
        let requested: Vec<_> = affected
            .iter()
            .map(|(v, _)| (v.clone(), ModelVector { x: 1., y: 0. }))
            .collect();
        let solved = self.solve(&requested)?;
        ensure!(
            solved
                .iter()
                .zip(affected)
                .all(|(d, (_, w))| d.x.hypot(d.y) / w <= 1. / MIN_GAIN as f32 + 0.001),
            "Glue and BlendShape influence is too weak; adjust its constraints or use Edit original points"
        );
        Ok(())
    }
    /// Solve each connected selected group jointly. The reference displacement
    /// is the intended gesture: retaining its null-space component prevents a
    /// coincident pair from losing its hidden separation during a common move.
    pub fn solve(&self, desired: &[(Vertex, ModelVector)]) -> Result<Vec<ModelVector>> {
        let slots: HashMap<_, _> = desired
            .iter()
            .enumerate()
            .map(|(i, (v, _))| (v, i))
            .collect();
        let mut adjacency = vec![Vec::new(); desired.len()];
        for (i, (v, _)) in desired.iter().enumerate() {
            if let Some(row) = self.rows.get(v) {
                for key in row.keys() {
                    if let Some(&j) = slots.get(key) {
                        adjacency[i].push(j);
                        adjacency[j].push(i);
                    }
                }
            }
        }
        let mut result: Vec<_> = desired.iter().map(|(_, d)| *d).collect();
        let mut visited = HashSet::new();
        for start in 0..desired.len() {
            if !visited.insert(start) {
                continue;
            }
            let mut group = vec![start];
            let mut at = 0;
            while at < group.len() {
                for &j in &adjacency[group[at]] {
                    if visited.insert(j) {
                        group.push(j);
                    }
                }
                at += 1;
            }
            let mut basis: Vec<(Vec<f64>, [f64; 2])> = Vec::new();
            for &i in &group {
                let vertex = &desired[i].0;
                let mut row: Vec<_> = group
                    .iter()
                    .map(|&j| {
                        self.rows
                            .get(vertex)
                            .map_or(if i == j { 1. } else { 0. }, |r| {
                                r.get(&desired[j].0).copied().unwrap_or(0.)
                            })
                    })
                    .collect();
                let mut rhs = [f64::from(desired[i].1.x), f64::from(desired[i].1.y)];
                // Reorthogonalize for long chains and nearly dependent rows.
                for _ in 0..2 {
                    for (q, value) in &basis {
                        let dot: f64 = row.iter().zip(q).map(|(a, b)| a * b).sum();
                        for (a, b) in row.iter_mut().zip(q) {
                            *a -= dot * b;
                        }
                        for k in 0..2 {
                            rhs[k] -= dot * value[k];
                        }
                    }
                }
                let norm = row.iter().map(|v| v * v).sum::<f64>().sqrt();
                if norm < 1e-8 {
                    ensure!(
                        rhs[0].hypot(rhs[1]) < 0.001,
                        "Glue fixes vertex {}: select the controlling endpoint or use Edit original points",
                        vertex.1
                    );
                    continue;
                }
                for v in &mut row {
                    *v /= norm;
                }
                basis.push((row, [rhs[0] / norm, rhs[1] / norm]));
            }
            let reference: Vec<_> = group
                .iter()
                .map(|&i| [f64::from(desired[i].1.x), f64::from(desired[i].1.y)])
                .collect();
            let mut solved = reference.clone();
            for (q, value) in basis {
                for k in 0..2 {
                    let residual =
                        value[k] - q.iter().zip(&reference).map(|(a, b)| a * b[k]).sum::<f64>();
                    for (p, a) in solved.iter_mut().zip(&q) {
                        p[k] += a * residual;
                    }
                }
            }
            let limit = reference
                .iter()
                .map(|p| p[0].hypot(p[1]))
                .fold(0_f64, f64::max)
                / MIN_GAIN
                + 0.001;
            ensure!(
                solved.iter().all(|p| p[0].hypot(p[1]) <= limit),
                "Glue influence is too weak: use Edit original points"
            );
            for (i, p) in group.into_iter().zip(solved) {
                ensure!(p.iter().all(|v| v.is_finite()), "Invalid Glue inverse");
                result[i] = ModelVector {
                    x: p[0] as f32,
                    y: p[1] as f32,
                };
            }
        }
        Ok(result)
    }
}

pub(crate) fn positions(
    targets: &[crate::control_edit::ControlEdit],
    selected: &[crate::ComponentRef],
    change: crate::control_edit::ControlChange,
) -> Result<HashMap<usize, Vec<zipzi_core::Vec2>>> {
    let mut requested = Vec::new();
    let mut destinations = Vec::new();
    for (target_index, target) in targets.iter().enumerate() {
        let Some(mesh) = &target.mesh else {
            continue;
        };
        for (slot, &id) in target.ids.iter().enumerate() {
            if !change.moves_all()
                && !selected
                    .iter()
                    .any(|c| c.object == target.object && c.id == id)
            {
                continue;
            }
            let before = if mesh.original_points {
                target.to_model(mesh.local[slot])
            } else {
                mesh.displayed[slot]
            };
            let after = change.apply_to(before);
            requested.push((
                (target.object.id.clone(), id),
                crate::ModelVector {
                    x: after.x - before.x,
                    y: after.y - before.y,
                },
            ));
            destinations.push((target_index, slot));
        }
    }
    let mesh_target = targets.iter().find_map(|t| t.mesh.as_ref());
    let solved = if let Some(mesh) = mesh_target.filter(|m| !m.original_points) {
        mesh.glue.solve(&requested)?
    } else {
        requested.iter().map(|(_, d)| *d).collect()
    };
    let max_requested = requested
        .iter()
        .map(|(_, d)| d.x.hypot(d.y))
        .fold(0_f32, f32::max);
    for (&(target, slot), delta) in destinations.iter().zip(&solved) {
        let mesh = targets[target].mesh.as_ref().unwrap();
        ensure!(
            delta.x.hypot(delta.y) / mesh.weight <= max_requested * 20. + 0.001,
            "Combined Glue and BlendShape influence is too weak at vertex {}: adjust the constraint or use Edit original points",
            targets[target].ids[slot]
        );
    }
    let mut mesh_positions = std::collections::HashMap::new();
    for (index, target) in targets.iter().enumerate().filter(|(_, t)| t.mesh.is_some()) {
        let displacement: Vec<_> = destinations
            .iter()
            .zip(&solved)
            .filter_map(|(&(i, slot), &delta)| (i == index).then_some((slot, delta)))
            .collect();
        mesh_positions.insert(index, target.mesh_positions(&displacement)?);
    }
    Ok(mesh_positions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_edit::{ControlChange, ControlDescription, ControlEdit};
    use crate::{CanvasScene, ComponentRef, Document, ObjectKind, ObjectRef};
    use zipzi_core::*;

    fn id(n: u128) -> String {
        uuid::Uuid::from_u128(n).to_string()
    }
    fn op(a: u32, b: u32, wa: f32, wb: f32) -> evaluation::GlueOperation {
        evaluation::GlueOperation {
            mesh_a: id(1),
            vertex_a: a,
            mesh_b: id(1),
            vertex_b: b,
            amount_a: wa,
            amount_b: wb,
        }
    }
    fn d(x: f32, y: f32) -> ModelVector {
        ModelVector { x, y }
    }
    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 0.003, "{a} != {b}");
    }

    #[test]
    fn glue_inverse_preserves_shared_gesture_and_rejects_weak_authority() -> anyhow::Result<()> {
        let map = GlueMap::new(&[op(0, 1, 0.5, 0.5)]);
        let solved = map.solve(&[((id(1), 0), d(10., 4.)), ((id(1), 1), d(10., 4.))])?;
        assert_eq!(solved, vec![d(10., 4.); 2]);
        let one = map.solve(&[((id(1), 0), d(10., 4.))])?;
        assert_eq!(one, vec![d(20., 8.)]);
        assert!(
            map.solve(&[((id(1), 0), d(10., 4.)), ((id(1), 1), d(-10., 4.))])
                .is_err()
        );
        for amount in [1., 0.99] {
            let map = GlueMap::new(&[op(0, 1, amount, 0.)]);
            assert!(map.solve(&[((id(1), 0), d(1., 0.))]).is_err());
        }
        let map = GlueMap::new(&[op(0, 1, 1., 0.)]);
        assert!(map.check_drag(&[((id(1), 0), 1.)], false).is_err());
        assert!(
            map.check_drag(&[((id(1), 0), 1.), ((id(1), 1), 1.)], false)
                .is_ok()
        );
        assert!(map.check_drag(&[((id(1), 0), 1.)], true).is_ok());
        Ok(())
    }

    #[test]
    fn glue_inverse_matches_ordered_chains_and_self_pairs() -> anyhow::Result<()> {
        let ops = [
            op(0, 1, 0.4, 0.3),
            op(1, 2, 0.2, 0.5),
            op(0, 2, 0.1, 0.1),
            op(2, 2, 0.9, 0.9),
        ];
        let map = GlueMap::new(&ops);
        let solved = map.solve(&[((id(1), 0), d(7., -2.)), ((id(1), 2), d(7., -2.))])?;
        let mut points = [
            Vec2::new(solved[0].x, solved[0].y),
            Vec2::default(),
            Vec2::new(solved[1].x, solved[1].y),
        ];
        for op in ops {
            let a = op.vertex_a as usize;
            let b = op.vertex_b as usize;
            let (pa, pb) = op.apply(points[a], points[b]);
            points[a] = pa;
            points[b] = pb;
        }
        for i in [0, 2] {
            close(points[i].x, 7.);
            close(points[i].y, -2.);
        }
        Ok(())
    }

    fn fixture(amount: f32, blend: bool) -> anyhow::Result<(Document, tempfile::TempDir)> {
        fixture_with_parent(amount, blend, false)
    }
    fn fixture_with_parent(
        amount: f32,
        blend: bool,
        parent: bool,
    ) -> anyhow::Result<(Document, tempfile::TempDir)> {
        fixture_with_setup(amount, blend, parent, |_| Ok(()))
    }
    fn fixture_with_setup(
        amount: f32,
        blend: bool,
        parent: bool,
        setup: impl FnOnce(&mut zipzi_sdk::EditSession<'_>) -> Result<(), zipzi_sdk::SdkError>,
    ) -> anyhow::Result<(Document, tempfile::TempDir)> {
        let dir = tempfile::tempdir()?;
        let mut s = zipzi_sdk::AuthoringSession::new(
            &id(90),
            Canvas::new(256., 256., Vec2::default(), 1.),
        )?;
        s.edit("fixture", None, |e| {
            if parent {
                e.create_transform(Transform {
                    id: id(12),
                    name: "Parent".into(),
                    data: TransformData::Warp(WarpTransform {
                        rows: 1,
                        columns: 1,
                        quad: false,
                        bezier: None,
                        points: vec![
                            Vec2::new(0., 0.),
                            Vec2::new(200., 0.),
                            Vec2::new(0., 100.),
                            Vec2::new(250., 220.),
                        ],
                    }),
                    ..Default::default()
                })?;
            }
            let positions = vec![
                Vec2::new(10., 10.),
                Vec2::new(110., 10.),
                Vec2::new(10., 110.),
            ];
            for n in [1, 2] {
                e.create_mesh(Mesh {
                    id: id(n),
                    name: format!("Mesh {n}"),
                    runtime_id: format!("mesh{n}"),
                    vertex_ids: vec![10, 20, 30],
                    base_positions: if parent && n == 1 {
                        vec![
                            Vec2::new(0.1, 0.1),
                            Vec2::new(0.8, 0.1),
                            Vec2::new(0.1, 0.8),
                        ]
                    } else {
                        positions.clone()
                    },
                    deformer_id: if parent && n == 1 {
                        id(12)
                    } else {
                        String::new()
                    },
                    uvs: vec![Vec2::default(); 3],
                    triangles: vec![[10, 20, 30]],
                    ..Default::default()
                })?;
            }
            e.create_glue(Glue {
                id: id(3),
                runtime_id: "seam".into(),
                name: "Seam".into(),
                mesh_a_id: id(1).into(),
                mesh_b_id: id(2).into(),
                intensity: 1.,
                binding: None,
                pairs: vec![GlueVertexPair {
                    vertex_a: Some(10),
                    vertex_b: Some(10),
                    weight_a: amount,
                    weight_b: 0.,
                    ..Default::default()
                }],
            })?;
            if blend {
                for (n, name, kind) in [
                    (4, "Pose", ParameterKind::Normal),
                    (5, "Smile", ParameterKind::BlendShape),
                    (8, "Constraint", ParameterKind::Normal),
                ] {
                    e.create_parameter(Parameter {
                        id: id(n),
                        name: name.into(),
                        minimum: 0.,
                        maximum: 1.,
                        kind,
                        ..Default::default()
                    })?;
                }
                e.create_binding(MeshBinding {
                    id: id(9),
                    mesh_id: id(1),
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
                            positions: positions
                                .iter()
                                .map(|p| Vec2::new(p.x + 20., p.y))
                                .collect(),
                            ..Default::default()
                        },
                    ],
                })?;
                e.create_blend_key_table(BlendShapeKeyTable {
                    id: id(6),
                    parameter_id: id(5),
                    keys: vec![0., 1.],
                    base_key_idx: 0,
                    samples: Default::default(),
                    base_sample_id: Default::default(),
                })?;
                e.create_blend_constraint(BlendShapeConstraint {
                    id: id(11),
                    parameter_id: id(8),
                    keys: vec![0., 1.],
                    weights: vec![1., 0.],
                    samples: vec![],
                })?;
                e.create_blend_binding(BlendShapeBinding {
                    id: id(7),
                    target_id: id(1),
                    target_kind: BlendShapeTargetKind::Mesh,
                    key_table_id: id(6),
                    constraint_ids: vec![id(11)],
                    keyforms: DeltaKeyforms::Mesh(vec![
                        DeltaMeshKeyform {
                            positions: vec![Vec2::default(); 3],
                            ..Default::default()
                        },
                        DeltaMeshKeyform {
                            positions: vec![Vec2::new(2., 3.); 3],
                            ..Default::default()
                        },
                    ]),
                })?;
            }
            setup(e)
        })?;
        let receipt = s.save_project(&dir.path().join("fixture.zipzi"), None)?;
        let mut doc = Document::open(&receipt.manifest)?;
        doc.select(&id(1))?;
        Ok((doc, dir))
    }
    fn capture_scene(doc: &mut Document) -> anyhow::Result<CanvasScene> {
        let source = doc.snapshot()?;
        Ok(CanvasScene::new(doc, source))
    }
    fn target() -> ObjectRef {
        ObjectRef::new(ObjectKind::Mesh, id(1))
    }
    fn component() -> Vec<ComponentRef> {
        vec![ComponentRef {
            object: target(),
            id: 10,
        }]
    }
    fn preview(doc: &mut Document, edit: &ControlEdit, dx: f32) -> anyhow::Result<()> {
        doc.preview_control_edit(
            std::slice::from_ref(edit),
            &component(),
            ControlChange::Translate(d(dx, 4.)),
        )
    }

    #[test]
    fn glue_preview_writes_only_selected_source_and_has_one_undo_or_cancel() -> anyhow::Result<()> {
        for cancel in [false, true] {
            let (mut doc, _dir) = fixture(0.5, false)?;
            let scene = capture_scene(&mut doc)?;
            let original = doc.model().get_mesh(&id(1)).unwrap().clone();
            let peer = doc.model().get_mesh(&id(2)).unwrap().clone();
            let edit = ControlEdit::prepare(&doc, &scene, &target())?;
            doc.begin_control_edit(std::slice::from_ref(&edit))?;
            for dx in [3., 6., 12.] {
                preview(&mut doc, &edit, dx)?;
            }
            let accepted = doc.model().get_mesh(&id(1)).unwrap().clone();
            let failure = preview(&mut doc, &edit, f32::MAX).unwrap_err();
            assert!(failure.is::<PreviewConstraint>());
            assert_eq!(doc.model().get_mesh(&id(1)), Some(&accepted));
            let actual = doc
                .snapshot()?
                .viewport_frame
                .drawables
                .iter()
                .find(|m| m.id == id(1))
                .unwrap()
                .positions[0];
            close(actual.x, scene.mesh(&id(1)).unwrap().vertices[0].1.x + 12.);
            close(
                doc.model().get_mesh(&id(1)).unwrap().base_positions[0].x,
                original.base_positions[0].x + 24.,
            );
            assert_eq!(doc.model().get_mesh(&id(2)), Some(&peer));
            assert_eq!(
                doc.model().get_mesh(&id(1)).unwrap().base_positions[1..],
                original.base_positions[1..]
            );
            if cancel {
                doc.cancel_edit();
                assert_eq!(doc.history_lengths(), (0, 0));
            } else {
                doc.finish_edit()?;
                assert_eq!(doc.history_lengths(), (1, 0));
                doc.undo()?;
            }
            assert_eq!(doc.model().get_mesh(&id(1)), Some(&original));
        }
        Ok(())
    }

    #[test]
    fn mesh_capabilities_keep_pose_weights_and_reject_weak_blend_authority() -> anyhow::Result<()> {
        let (mut doc, _dir) = fixture(0.5, true)?;
        doc.focus_parameter(&id(5))?;
        doc.set_preview_parameters(&[(id(4), 0.35), (id(5), 1.), (id(8), 0.5)])?;
        let retained = capture_scene(&mut doc)?;
        let ControlDescription::Writable(writable) =
            ControlEdit::describe(&doc, &retained, &target())?
        else {
            panic!(
                "a stored BlendShape keyform must remain writable over an interpolated ordinary pose"
            );
        };
        assert_eq!(writable.mesh_weight, Some(0.5));
        assert!(writable.label.contains("Smile"));
        doc.set_preview_parameter(&id(8), 0.98)?;
        let weak = capture_scene(&mut doc)?;
        let ControlDescription::ReadOnly(reason) = ControlEdit::describe(&doc, &weak, &target())?
        else {
            panic!("a weak BlendShape must not expose editable vertices");
        };
        assert!(reason.contains("insufficient effective influence"));
        let ControlDescription::Writable(writable) =
            ControlEdit::describe(&doc, &retained, &target())?
        else {
            panic!("retained poses must retain their own effective weights");
        };
        assert_eq!(writable.mesh_weight, Some(0.5));
        doc.set_preview_parameters(&[(id(4), 0.), (id(5), 0.)])?;
        let neutral = capture_scene(&mut doc)?;
        let ControlDescription::Writable(writable) =
            ControlEdit::describe(&doc, &neutral, &target())?
        else {
            panic!("neutral BlendShapes must allow the stored ordinary form");
        };
        assert_eq!(writable.mesh_weight, Some(1.));
        Ok(())
    }

    #[test]
    fn blend_delta_inverse_composes_glue_constraints_and_interpolated_ordinary_pose()
    -> anyhow::Result<()> {
        let (mut doc, _dir) = fixture(0.5, true)?;
        doc.focus_parameter(&id(5))?;
        doc.set_preview_parameters(&[(id(4), 0.35), (id(5), 1.), (id(8), 0.5)])?;
        let scene = capture_scene(&mut doc)?;
        assert!(matches!(
            ControlEdit::describe(&doc, &scene, &target())?,
            ControlDescription::Writable(_)
        ));
        let original = doc.model().get_blend_binding(&id(7)).unwrap().clone();
        let ordinary = doc.model().get_binding(&id(9)).unwrap().clone();
        let base = doc.model().get_mesh(&id(1)).unwrap().clone();
        let edit = ControlEdit::prepare(&doc, &scene, &target())?;
        doc.begin_control_edit(std::slice::from_ref(&edit))?;
        preview(&mut doc, &edit, 10.)?;
        let actual = doc
            .snapshot()?
            .viewport_frame
            .drawables
            .iter()
            .find(|m| m.id == id(1))
            .unwrap()
            .positions[0];
        close(actual.x, scene.mesh(&id(1)).unwrap().vertices[0].1.x + 10.);
        close(actual.y, scene.mesh(&id(1)).unwrap().vertices[0].1.y + 4.);
        let binding = doc.model().get_blend_binding(&id(7)).unwrap().clone();
        let DeltaKeyforms::Mesh(forms) = binding.keyforms else {
            panic!()
        };
        close(forms[1].positions[0].x, 42.); // delta 10 / Glue .5 / constraint .5
        assert_eq!(forms[0].positions, vec![Vec2::default(); 3]);
        assert_eq!(forms[1].positions[1..], vec![Vec2::new(2., 3.); 2]);
        assert_eq!(doc.model().get_binding(&id(9)), Some(&ordinary));
        assert_eq!(doc.model().get_mesh(&id(1)), Some(&base));
        doc.finish_edit()?;
        doc.undo()?;
        assert_eq!(doc.model().get_blend_binding(&id(7)), Some(&original));
        // Zero effective weight cannot be inverted, and the hint names the parameter.
        doc.set_preview_parameter(&id(8), 1.)?;
        let scene = capture_scene(&mut doc)?;
        let error = ControlEdit::prepare(&doc, &scene, &target())
            .err()
            .unwrap()
            .to_string();
        assert!(
            error.contains("Smile") && error.contains("constraints"),
            "{error}"
        );
        Ok(())
    }

    #[test]
    fn glue_fixed_endpoint_can_edit_original_points_without_changing_peer() -> anyhow::Result<()> {
        let (mut doc, _dir) = fixture(1., false)?;
        let scene = capture_scene(&mut doc)?;
        let mut edit = ControlEdit::prepare(&doc, &scene, &target())?;
        doc.begin_control_edit(std::slice::from_ref(&edit))?;
        assert!(preview(&mut doc, &edit, 10.).is_err());
        doc.cancel_edit();
        edit.mesh.as_mut().unwrap().original_points = true;
        doc.begin_control_edit(std::slice::from_ref(&edit))?;
        preview(&mut doc, &edit, 10.)?;
        close(
            doc.model().get_mesh(&id(1)).unwrap().base_positions[0].x,
            20.,
        );
        close(
            doc.model().get_mesh(&id(2)).unwrap().base_positions[0].x,
            10.,
        );
        close(
            doc.snapshot()?.viewport_frame.drawables[0].positions[0].x,
            10.,
        );
        doc.finish_edit()?;
        Ok(())
    }
    #[test]
    fn glue_joint_mesh_targets_preserve_the_common_translation() -> anyhow::Result<()> {
        let (mut doc, _dir) = fixture(0.5, false)?;
        let targets = vec![target(), ObjectRef::new(ObjectKind::Mesh, id(2))];
        doc.select_objects(targets.clone())?;
        let scene = capture_scene(&mut doc)?;
        let edits = targets
            .iter()
            .map(|t| ControlEdit::prepare(&doc, &scene, t))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let selected = targets
            .into_iter()
            .map(|object| ComponentRef { object, id: 10 })
            .collect::<Vec<_>>();
        doc.begin_control_edit(&edits)?;
        doc.preview_control_edit(&edits, &selected, ControlChange::Translate(d(10., 4.)))?;
        for n in [1, 2] {
            close(
                doc.model().get_mesh(&id(n)).unwrap().base_positions[0].x,
                20.,
            );
        }
        doc.finish_edit()?;
        assert_eq!(doc.history_lengths(), (1, 0));
        doc.undo()?;
        for n in [1, 2] {
            close(
                doc.model().get_mesh(&id(n)).unwrap().base_positions[0].x,
                10.,
            );
        }
        Ok(())
    }
    #[test]
    fn glue_inverse_composes_with_a_nonlinear_parent_warp() -> anyhow::Result<()> {
        let (mut doc, _dir) = fixture_with_parent(0.5, false, true)?;
        let scene = capture_scene(&mut doc)?;
        let parent = doc.model().get_transform(&id(12)).unwrap().clone();
        let edit = ControlEdit::prepare(&doc, &scene, &target())?;
        doc.begin_control_edit(std::slice::from_ref(&edit))?;
        preview(&mut doc, &edit, 10.)?;
        let actual = doc.snapshot()?.viewport_frame.drawables[0].positions[0];
        close(actual.x, scene.mesh(&id(1)).unwrap().vertices[0].1.x + 10.);
        close(actual.y, scene.mesh(&id(1)).unwrap().vertices[0].1.y + 4.);
        assert_eq!(doc.model().get_transform(&id(12)), Some(&parent));
        doc.cancel_edit();
        Ok(())
    }
    #[test]
    fn blend_zero_base_keyform_edits_with_other_blends_and_glue() -> anyhow::Result<()> {
        for through_binding in [false, true] {
            let (mut doc, _dir) = fixture_with_setup(0.5, true, false, |e| {
                e.create_parameter(Parameter {
                    id: id(14),
                    name: "Other blend".into(),
                    minimum: 0.,
                    maximum: 1.,
                    kind: ParameterKind::BlendShape,
                    ..Default::default()
                })?;
                e.create_blend_key_table(BlendShapeKeyTable {
                    id: id(15),
                    parameter_id: id(14),
                    keys: vec![0., 1.],
                    base_key_idx: 0,
                    samples: vec![],
                    base_sample_id: String::new(),
                })?;
                e.create_blend_binding(BlendShapeBinding {
                    id: id(16),
                    target_id: id(1),
                    target_kind: BlendShapeTargetKind::Mesh,
                    key_table_id: id(15),
                    constraint_ids: vec![],
                    keyforms: DeltaKeyforms::Mesh(vec![
                        DeltaMeshKeyform {
                            positions: vec![Vec2::default(); 3],
                            ..Default::default()
                        },
                        DeltaMeshKeyform {
                            positions: vec![Vec2::new(6., 8.); 3],
                            ..Default::default()
                        },
                    ]),
                })
            })?;
            if through_binding {
                let binding = ObjectRef::new(ObjectKind::BlendBinding, id(7));
                doc.open_binding(binding.clone())?;
                doc.preview_keyform(&binding, 0)?;
            } else {
                doc.focus_parameter(&id(5))?;
                doc.set_preview_parameter(&id(5), 0.)?;
            }
            doc.set_preview_parameters(&[(id(4), 0.), (id(14), 1.)])?;
            let scene = capture_scene(&mut doc)?;
            assert!(matches!(
                ControlEdit::describe(&doc, &scene, &target())?,
                ControlDescription::Writable(_)
            ));
            let ordinary = doc.model().get_binding(&id(9)).unwrap().clone();
            let mesh = doc.model().get_mesh(&id(1)).unwrap().clone();
            let blends: Vec<_> = [7, 16]
                .into_iter()
                .map(|n| doc.model().get_blend_binding(&id(n)).unwrap().clone())
                .collect();
            let edit = ControlEdit::prepare(&doc, &scene, &target())?;
            assert!(matches!(
                edit.original,
                crate::control_edit::EditOriginal::MeshBinding(_)
            ));
            doc.begin_control_edit(std::slice::from_ref(&edit))?;
            preview(&mut doc, &edit, 10.)?;
            let actual = doc.snapshot()?.viewport_frame.drawables[0].positions[0];
            close(actual.x, scene.mesh(&id(1)).unwrap().vertices[0].1.x + 10.);
            close(actual.y, scene.mesh(&id(1)).unwrap().vertices[0].1.y + 4.);
            assert_eq!(doc.model().get_mesh(&id(1)), Some(&mesh));
            for b in &blends {
                assert_eq!(doc.model().get_blend_binding(&b.id), Some(b));
            }
            assert_eq!(
                doc.model().get_binding(&id(9)).unwrap().keyforms[1],
                ordinary.keyforms[1]
            );
            doc.finish_edit()?;
            assert_eq!(doc.history_lengths(), (1, 0));
            doc.undo()?;
            assert_eq!(doc.model().get_binding(&id(9)), Some(&ordinary));
            doc.redo()?;
            let moved = doc.model().get_binding(&id(9)).unwrap().keyforms[0].positions[0];
            close(moved.x, ordinary.keyforms[0].positions[0].x + 20.);
            close(moved.y, ordinary.keyforms[0].positions[0].y + 8.);
            // The ordinary destination still has to be an actual stored form.
            doc.set_preview_parameter(&id(4), 0.35)?;
            let off_key = capture_scene(&mut doc)?;
            let error = ControlEdit::prepare(&doc, &off_key, &target())
                .err()
                .unwrap()
                .to_string();
            assert!(
                error.contains("Pose") && error.contains("stored keyform"),
                "{error}"
            );
        }
        Ok(())
    }

    #[test]
    fn blend_zero_base_keyform_allows_warp_and_rotation_base_edits() -> anyhow::Result<()> {
        for rotation in [false, true] {
            let (mut doc, _dir) = fixture_with_setup(0., true, false, |e| {
                let points = vec![
                    Vec2::new(10., 10.),
                    Vec2::new(110., 10.),
                    Vec2::new(10., 110.),
                    Vec2::new(110., 110.),
                ];
                e.create_transform(Transform {
                    id: id(20),
                    name: "Blend deformer".into(),
                    data: if rotation {
                        TransformData::Rotation(RotationTransform::default())
                    } else {
                        TransformData::Warp(WarpTransform {
                            rows: 1,
                            columns: 1,
                            quad: true,
                            bezier: None,
                            points: points.clone(),
                        })
                    },
                    ..Default::default()
                })?;
                e.create_blend_binding(BlendShapeBinding {
                    id: id(21),
                    target_id: id(20),
                    key_table_id: id(6),
                    constraint_ids: vec![],
                    target_kind: if rotation {
                        BlendShapeTargetKind::Rotation
                    } else {
                        BlendShapeTargetKind::Warp
                    },
                    keyforms: if rotation {
                        DeltaKeyforms::Rotation(vec![
                            DeltaRotationKeyform::default(),
                            DeltaRotationKeyform {
                                origin: Some(Vec2::new(2., 3.)),
                                ..Default::default()
                            },
                        ])
                    } else {
                        DeltaKeyforms::Warp(vec![
                            DeltaWarpKeyform {
                                points: vec![Vec2::default(); 4],
                                ..Default::default()
                            },
                            DeltaWarpKeyform {
                                points: vec![Vec2::new(2., 3.); 4],
                                ..Default::default()
                            },
                        ])
                    },
                })
            })?;
            let object = ObjectRef::new(ObjectKind::Deformer, id(20));
            doc.select_object(object.clone())?;
            doc.focus_parameter(&id(5))?;
            doc.set_preview_parameter(&id(5), 0.)?;
            let scene = capture_scene(&mut doc)?;
            assert!(matches!(
                ControlEdit::describe(&doc, &scene, &object)?,
                ControlDescription::Writable(_)
            ));
            let base = doc.model().get_transform(&id(20)).unwrap().clone();
            let blend = doc.model().get_blend_binding(&id(21)).unwrap().clone();
            let edit = ControlEdit::prepare(&doc, &scene, &object)?;
            doc.begin_control_edit(std::slice::from_ref(&edit))?;
            doc.preview_control_edit(
                std::slice::from_ref(&edit),
                &[],
                ControlChange::MoveAll(d(10., 4.)),
            )?;
            assert_ne!(doc.model().get_transform(&id(20)), Some(&base));
            assert_eq!(doc.model().get_blend_binding(&id(21)), Some(&blend));
            doc.finish_edit()?;
            assert_eq!(doc.history_lengths(), (1, 0));
            doc.undo()?;
            assert_eq!(doc.model().get_transform(&id(20)), Some(&base));
            if !rotation {
                // Bezier handles refine the conversion lattice. Existing blend
                // deltas must follow that topology, including undo/cancel.
                for cancel in [true, false] {
                    let scene = capture_scene(&mut doc)?;
                    let edit = ControlEdit::prepare(&doc, &scene, &object)?;
                    doc.begin_control_edit(std::slice::from_ref(&edit))?;
                    let selected = [ComponentRef {
                        object: object.clone(),
                        id: crate::warp_bezier::component_id(0, 1),
                    }];
                    doc.preview_control_edit(
                        std::slice::from_ref(&edit),
                        &selected,
                        ControlChange::Translate(d(10., 4.)),
                    )?;
                    let model = doc.model();
                    let warp = model.get_transform(&id(20)).unwrap().warp().unwrap();
                    assert_eq!((warp.rows, warp.columns), (4, 4));
                    let DeltaKeyforms::Warp(forms) =
                        &model.get_blend_binding(&id(21)).unwrap().keyforms
                    else {
                        panic!()
                    };
                    assert_eq!(forms[0].points, vec![Vec2::default(); 25]);
                    for p in &forms[1].points {
                        close(p.x, 2.);
                        close(p.y, 3.);
                    }
                    drop(model);
                    if cancel {
                        doc.cancel_edit();
                    } else {
                        doc.finish_edit()?;
                        doc.undo()?;
                    }
                    assert_eq!(doc.model().get_transform(&id(20)), Some(&base));
                    assert_eq!(doc.model().get_blend_binding(&id(21)), Some(&blend));
                }
            }
        }
        Ok(())
    }
    #[test]
    fn blend_initial_target_uses_only_an_unambiguous_stored_keyform() -> anyhow::Result<()> {
        for state in ["stored", "between_keys", "ambiguous", "constrained"] {
            let (mut doc, _dir) = fixture_with_setup(0.5, true, false, |e| {
                let mut p = e
                    .candidate_document()
                    .get_parameter(&id(5))
                    .unwrap()
                    .clone();
                p.default_value = if state == "between_keys" { 0.5 } else { 1. };
                e.replace_parameter(p)?;
                if state == "constrained" {
                    let mut p = e
                        .candidate_document()
                        .get_parameter(&id(8))
                        .unwrap()
                        .clone();
                    p.default_value = 0.99;
                    e.replace_parameter(p)?;
                }
                if state == "ambiguous" {
                    e.create_parameter(Parameter {
                        id: id(14),
                        name: "Other blend".into(),
                        minimum: 0.,
                        maximum: 1.,
                        default_value: 1.,
                        kind: ParameterKind::BlendShape,
                        ..Default::default()
                    })?;
                    e.create_blend_key_table(BlendShapeKeyTable {
                        id: id(15),
                        parameter_id: id(14),
                        keys: vec![0., 1.],
                        base_key_idx: 0,
                        samples: vec![],
                        base_sample_id: String::new(),
                    })?;
                    e.create_blend_binding(BlendShapeBinding {
                        id: id(16),
                        target_id: id(1),
                        target_kind: BlendShapeTargetKind::Mesh,
                        key_table_id: id(15),
                        constraint_ids: vec![],
                        keyforms: DeltaKeyforms::Mesh(vec![
                            DeltaMeshKeyform {
                                positions: vec![Vec2::default(); 3],
                                ..Default::default()
                            },
                            DeltaMeshKeyform {
                                positions: vec![Vec2::new(6., 8.); 3],
                                ..Default::default()
                            },
                        ]),
                    })?;
                }
                Ok(())
            })?;
            assert!(doc.context().parameter_focus.is_none());
            assert!(doc.context().active_binding.is_none());
            let scene = capture_scene(&mut doc)?;
            if state != "stored" {
                let error = ControlEdit::prepare(&doc, &scene, &target())
                    .err()
                    .unwrap()
                    .to_string();
                assert!(error.contains("Smile"), "{state}: {error}");
                if state == "ambiguous" {
                    assert!(error.contains("Other blend"), "{error}");
                }
                if state == "constrained" {
                    assert!(error.contains("constraints"), "{error}");
                }
                assert!(matches!(
                    ControlEdit::describe(&doc, &scene, &target())?,
                    ControlDescription::ReadOnly(_)
                ));
                assert_eq!(doc.history_lengths(), (0, 0));
                assert!(!doc.has_pending_edit());
                continue;
            }
            let edit = ControlEdit::prepare(&doc, &scene, &target())?;
            let crate::control_edit::EditOriginal::BlendBinding(original) = &edit.original else {
                panic!("initial target must be the active delta")
            };
            assert_eq!(original.id, id(7));
            assert_eq!(edit.slot, Some(1));
            assert!(
                matches!(ControlEdit::describe(&doc,&scene,&target())?,ControlDescription::Writable(w) if w.label.contains("Smile = 1"))
            );
            let base = doc.model().get_mesh(&id(1)).unwrap().clone();
            let ordinary = doc.model().get_binding(&id(9)).unwrap().clone();
            doc.begin_control_edit(std::slice::from_ref(&edit))?;
            preview(&mut doc, &edit, 10.)?;
            let actual = doc.snapshot()?.viewport_frame.drawables[0].positions[0];
            close(actual.x, scene.mesh(&id(1)).unwrap().vertices[0].1.x + 10.);
            close(actual.y, scene.mesh(&id(1)).unwrap().vertices[0].1.y + 4.);
            assert_eq!(doc.model().get_mesh(&id(1)), Some(&base));
            assert_eq!(doc.model().get_binding(&id(9)), Some(&ordinary));
            doc.finish_edit()?;
            assert_eq!(doc.history_lengths(), (1, 0));
            doc.undo()?;
            assert_eq!(doc.model().get_blend_binding(&id(7)), Some(original));
            doc.focus_parameter(&id(5))?;
            let explicit = ControlEdit::prepare(&doc, &scene, &target())?;
            assert_eq!(explicit.original.id(), edit.original.id());
            assert_eq!(explicit.slot, edit.slot);
        }
        Ok(())
    }
}
