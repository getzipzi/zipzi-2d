use super::*;
use zipzi_contract::authoring::BindingAuthoringEdit as E;
use zipzi_core::{RotationPose, RotationTransform, Transform, TransformData};

use zipzi_sdk::{
    AxisReduction, ConversionLimits, ParameterRangeEdit, ParameterRangePolicy, ReparentEdit,
    ReparentStrategy,
};
const Q: &str = "00000000-0000-4000-8000-000000000406";
const R: &str = "00000000-0000-4000-8000-000000000407";
const T: &str = "00000000-0000-4000-8000-000000000408";

fn apply(s: &mut AuthoringSession, e: E) -> zipzi_sdk::Publication {
    let p = crate::support::binding(&s, e).unwrap();
    s.publish(p.1).unwrap()
}

fn bound() -> AuthoringSession {
    let mut s = fixture();
    let b = binding(&s);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter(parameter(false))?;
                e.create_binding(b)
            })
            .and_then(|value| {
                let prepared = draft.prepare("bind")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    s.take_events();
    s
}

fn frame(s: &AuthoringSession, p: f32, q: f32) -> zipzi_core::DrawableFrame {
    crate::support::frame(
        s.snapshot(),
        &[(PARAMETER_ID, p), (Q, q)]
            .into_iter()
            .filter(|(id, _)| s.snapshot().document().parameter(id).cloned().is_some())
            .map(|(id, v)| (id.into(), v))
            .collect(),
    )
    .unwrap()
}

fn grid_roundtrip(s: &AuthoringSession) {
    let saved = crate::support::roundtrip(s.snapshot());
    let restored = saved.document();
    assert_eq!(
        restored.hierarchy_order(),
        s.snapshot().document().hierarchy_order()
    );
    assert_eq!(
        restored.binding(BINDING_ID),
        s.snapshot().document().binding(BINDING_ID)
    );
    assert_eq!(
        restored.mesh(MESH_ID),
        s.snapshot().document().mesh(MESH_ID)
    );
}

fn rotation(s: &mut AuthoringSession, scale: f32) {
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: R.into(),
                        data: TransformData::Rotation(RotationTransform {
                            base_angle: 12.0,
                            pose: RotationPose {
                                angle: 23.0,
                                scale,
                                origin: zipzi_core::PreciseVec2::new(5.0, -3.0),
                                reflect_x: true,
                                ..Default::default()
                            },
                        }),
                        ..Default::default()
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("rotation")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}

fn parent_edit(strategy: ReparentStrategy, parent: &str) -> E {
    E::Reparent(ReparentEdit {
        placement: None,
        sources: vec![MESH_ID.into()],
        parent_id: parent.into(),
        strategy,
        tolerance: 0.001,
        relative_tolerance: 1e-6,
        context: Some(zipzi_sdk::ReparentContext {
            parameter_values: [(PARAMETER_ID.into(), 0.5)].into_iter().collect(),
            parameter_domain: if strategy == ReparentStrategy::PreserveParameterDomain {
                vec![zipzi_core::document::ConversionDomain {
                    parameter_id: PARAMETER_ID.into(),
                    minimum: 0.,
                    maximum: 1.,
                }]
            } else {
                vec![]
            },
            edit_time_seconds: 0.,
        }),
    })
}

mod mesh_grid;
mod parameter_range;
mod reparent;
mod reparent_preservation;
mod validation;
