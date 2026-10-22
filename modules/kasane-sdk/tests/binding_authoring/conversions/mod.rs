use super::*;
use kasane_core::{RotationPose, RotationTransform, Transform, TransformData};
use kasane_sdk::{
    AuthoringLimits, AxisReduction, BindingAuthoringEdit as E, BindingConversion,
    MeshGridEdit as G, ParameterRangeEdit, ParameterRangePolicy, ReparentEdit, ReparentStrategy,
};
const Q: &str = "00000000-0000-4000-8000-000000000406";
const R: &str = "00000000-0000-4000-8000-000000000407";
const T: &str = "00000000-0000-4000-8000-000000000408";

fn apply(s: &mut AuthoringSession, e: E) -> kasane_sdk::EditReceipt {
    let p = s.plan_binding_authoring(e).unwrap();
    s.execute_binding_plan(&p).unwrap()
}

fn bound() -> AuthoringSession {
    let mut s = fixture();
    let b = binding(&s);
    s.edit("bind", None, |e| {
        e.create_parameter(parameter(false))?;
        e.create_binding(b)
    })
    .unwrap();
    s.drain_events();
    s
}

fn frame(s: &AuthoringSession, p: f32, q: f32) -> kasane_core::DrawableFrame {
    s.evaluate(
        &[(PARAMETER_ID, p), (Q, q)]
            .into_iter()
            .filter(|(id, _)| s.parameter(id).is_some())
            .map(|(id, v)| (id.into(), v))
            .collect(),
    )
    .unwrap()
}

fn grid_roundtrip(s: &AuthoringSession) {
    let mut document = s.document().fork_candidate();
    let mut asset = document.get_asset(ASSET_ID).unwrap().clone();
    asset.source = "assets/texture.png".into();
    assert!(document.replace_asset(asset).status.is_ok());
    let text = kasane_project::encode_project(&document).unwrap();
    let restored = kasane_project::decode_project(&text).unwrap();
    assert!(restored.validate_structure().is_empty());
    assert_eq!(
        restored.get_binding(BINDING_ID),
        s.document().get_binding(BINDING_ID)
    );
    assert_eq!(restored.get_mesh(MESH_ID), s.document().get_mesh(MESH_ID));
}

fn rotation(s: &mut AuthoringSession, scale: f32) {
    s.edit("rotation", None, |e| {
        e.create_transform(Transform {
            id: R.into(),
            data: TransformData::Rotation(RotationTransform {
                base_angle: 12.0,
                pose: RotationPose {
                    angle: 23.0,
                    scale,
                    origin: kasane_core::PreciseVec2::new(5.0, -3.0),
                    reflect_x: true,
                    ..Default::default()
                },
            }),
            ..Default::default()
        })
    })
    .unwrap();
}

fn parent_edit(strategy: ReparentStrategy, parent: &str) -> E {
    E::Reparent(ReparentEdit {
        sources: vec![MESH_ID.into()],
        parent_id: parent.into(),
        strategy,
        tolerance: 0.001,
    })
}

mod mesh_grid;
mod parameter_range;
mod reparent;
mod validation;
