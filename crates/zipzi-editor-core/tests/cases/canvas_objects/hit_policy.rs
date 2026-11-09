use super::common::*;
use crate::support::present_canvas as shown;
use zipzi_editor_core::*;

#[test]
fn selected_mesh_surface_wins_over_the_front_mesh() -> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    let frame = shown(&mut editor)?;
    let center = ModelPoint { x: 128., y: 128. };
    let point = frame.transform.to_viewport(center);
    let meshes = frame.geometry.mesh_hits(center);
    assert_eq!(meshes.len(), 2);
    let front = ObjectRef::new(ObjectKind::Mesh, &meshes[0]);
    let back = ObjectRef::new(ObjectKind::Mesh, &meshes[1]);

    editor.execute(EditorCommand::SelectObject(back.clone()))?;
    let picks = editor.pick_candidates(point, Some(frame.version));
    assert_eq!(picks[0].object, back);
    assert_eq!(picks[1].object, front);
    click(&mut editor, point, false)?;
    assert_eq!(
        editor.document().unwrap().selected_id(),
        Some(back.id.as_str())
    );

    editor.execute(EditorCommand::SelectObjects(Vec::new()))?;
    click(&mut editor, point, false)?;
    assert_eq!(
        editor.document().unwrap().selected_id(),
        Some(front.id.as_str())
    );
    Ok(())
}

#[test]
fn selected_mesh_surface_wins_over_an_unselected_rotation() -> anyhow::Result<()> {
    let (mut doc, _assets, _, rotation, _) = rich_document()?;
    let mesh = doc.context().selection.clone().unwrap();
    let rotation = ObjectRef::new(ObjectKind::Deformer, rotation);
    let source = pixel_scene(&mut doc)?;
    let scene = CanvasScene::new(&doc, source);
    let transform = ViewportTransform::new(
        ViewportMetrics {
            width: 512.,
            height: 512.,
            scale_factor: 1.,
        },
        ViewState::default(),
    )
    .unwrap();
    let control = scene
        .controls
        .iter()
        .find(|c| c.object == rotation)
        .unwrap();
    let points = scene.control_points(control, transform);
    let point = rotation_body_pointer(points[0], points[1]);
    let hit = |selection: &[ObjectRef]| {
        scene.hit(
            point,
            transform,
            selection,
            ToolId::Select,
            DeformerVisibility::default(),
        )
    };
    assert_eq!(hit(&[]), Some(HitTarget::Object(rotation)));
    assert_eq!(
        hit(std::slice::from_ref(&mesh)),
        Some(HitTarget::Object(mesh))
    );
    Ok(())
}

#[test]
fn selected_warp_wins_over_a_closer_warp_outline() -> anyhow::Result<()> {
    let (mut editor, _assets, warp) = deformer_editor(false)?;
    let frame = shown(&mut editor)?;
    let mut scene = (*frame.geometry).clone();
    let control = scene.controls.iter().find(|c| c.object == warp).unwrap();
    let (a, b) = scene.warp_outline(control, frame.transform)[0];
    let midpoint = ViewportPoint {
        x: (a.x + b.x) / 2.,
        y: (a.y + b.y) / 2.,
    };
    let point = ViewportPoint {
        y: midpoint.y - 2.,
        ..midpoint
    };

    let mut closer = control.clone();
    let other = crate::support::object(ObjectKind::Deformer, 7);
    closer.object = other.clone();
    let offset = frame.transform.to_model(point).y - frame.transform.to_model(midpoint).y;
    for p in &mut closer.points {
        p.y += offset;
    }
    scene.controls.push(closer);
    let visibility = DeformerVisibility {
        all: true,
        ..Default::default()
    };
    let hit = |selection: &[ObjectRef]| {
        scene.hit(
            point,
            frame.transform,
            selection,
            ToolId::Select,
            visibility,
        )
    };
    assert_eq!(hit(&[]), Some(HitTarget::Object(other)));
    assert_eq!(hit(std::slice::from_ref(&warp)).unwrap().object(), &warp);
    Ok(())
}

#[test]
fn point_and_line_hit_limits_are_screen_space_and_points_win() -> anyhow::Result<()> {
    let (mut editor, _assets, warp) = deformer_editor(false)?;
    let mesh = ObjectRef::new(ObjectKind::Mesh, uuid::Uuid::from_u128(6).to_string());
    for zoom in [0.25, 1., 4.] {
        editor.set_zoom_at(zoom, None);
        let frame = shown(&mut editor)?;
        let hit = |point, selection: &[ObjectRef], visibility| {
            frame.geometry.hit(
                point,
                frame.transform,
                selection,
                ToolId::Select,
                visibility,
            )
        };
        let control = frame
            .geometry
            .controls
            .iter()
            .find(|c| c.object == warp)
            .unwrap();
        let edges = frame.geometry.warp_outline(control, frame.transform);
        let (a, b) = edges[0];
        let middle = ViewportPoint {
            x: (a.x + b.x) / 2.,
            y: (a.y + b.y) / 2.,
        };
        // The first edge is horizontal, and neither press overlaps a control point.
        assert_eq!(
            hit(
                ViewportPoint {
                    y: middle.y + 3.,
                    ..middle
                },
                &[],
                DeformerVisibility::default()
            ),
            Some(HitTarget::Object(warp.clone()))
        );
        assert_ne!(
            hit(
                ViewportPoint {
                    y: middle.y + 3.1,
                    ..middle
                },
                &[],
                DeformerVisibility::default()
            ),
            Some(HitTarget::Object(warp.clone()))
        );

        let (id, point) = frame.geometry.mesh(&mesh.id).unwrap().vertices[0];
        let point = frame.transform.to_viewport(point);
        let visibility = DeformerVisibility {
            hide_unselected_warps: true,
            hide_unselected_rotations: true,
            ..Default::default()
        };
        let component = HitTarget::Component(ComponentRef {
            object: mesh.clone(),
            id,
        });
        assert_eq!(
            hit(
                ViewportPoint {
                    x: point.x - 6.,
                    ..point
                },
                std::slice::from_ref(&mesh),
                visibility
            ),
            Some(component.clone())
        );
        assert_ne!(
            hit(
                ViewportPoint {
                    x: point.x - 6.1,
                    ..point
                },
                std::slice::from_ref(&mesh),
                visibility
            ),
            Some(component.clone())
        );
        if zoom == 0.25 {
            // The point is also within three pixels of the Warp's left boundary.
            assert_eq!(
                hit(
                    ViewportPoint {
                        x: point.x - 2.,
                        ..point
                    },
                    std::slice::from_ref(&mesh),
                    DeformerVisibility::default()
                ),
                Some(component)
            );
        }
    }
    Ok(())
}

#[test]
fn unselected_rotation_surface_has_no_outside_padding() -> anyhow::Result<()> {
    let (mut editor, _assets, rotation) = deformer_editor(true)?;
    let frame = shown(&mut editor)?;
    let control = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object == rotation)
        .unwrap();
    let gizmo = frame
        .geometry
        .deformer_gizmo(control, frame.transform)
        .unwrap();
    let center = ViewportPoint {
        x: gizmo.outline.iter().map(|(p, _)| p.x).sum::<f32>() / 4.,
        y: gizmo.outline.iter().map(|(p, _)| p.y).sum::<f32>() / 4.,
    };
    let hit = |p| {
        frame.geometry.hit(
            p,
            frame.transform,
            &[],
            ToolId::Select,
            DeformerVisibility::default(),
        )
    };
    assert_eq!(hit(center), Some(HitTarget::Object(rotation.clone())));
    let (a, b) = gizmo.outline[1];
    let middle = ViewportPoint {
        x: (a.x + b.x) / 2.,
        y: (a.y + b.y) / 2.,
    };
    let (mut nx, mut ny) = (b.y - a.y, a.x - b.x);
    if nx * (middle.x - center.x) + ny * (middle.y - center.y) < 0. {
        nx = -nx;
        ny = -ny;
    }
    let length = nx.hypot(ny);
    assert_ne!(
        hit(ViewportPoint {
            x: middle.x + nx / length,
            y: middle.y + ny / length
        }),
        Some(HitTarget::Object(rotation))
    );
    Ok(())
}
