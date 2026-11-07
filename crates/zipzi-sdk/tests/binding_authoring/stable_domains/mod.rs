use super::*;
use zipzi_contract::authoring::BindingAuthoringEdit as E;
use zipzi_contract::authoring::ObjectKind;
use zipzi_core::*;
use zipzi_sdk::{
    AxisReduction as R, BindingGridEdit as G, BindingGridTarget as T, BlendAuthoringEdit as B,
    BlendTableOperation as BO, GridOperation as O,
};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn apply(s: &mut AuthoringSession, e: E) -> zipzi_sdk::Publication {
    let p = crate::support::binding(&s, e).unwrap();
    s.publish(p.1).unwrap()
}

fn base() -> AuthoringSession {
    let mut s = fixture();
    let b = binding(&s);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter(parameter(false))?;
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(1600),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.create_binding(b)
            })
            .and_then(|value| {
                let prepared = draft.prepare("parameters")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    s
}

fn op(s: &mut AuthoringSession, owner: &str, operation: O) -> zipzi_sdk::Publication {
    apply(
        s,
        E::Grid(G::Edit {
            owner: owner.into(),
            operation,
        }),
    )
}

fn roundtrip(s: &AuthoringSession) -> zipzi_core::state::DocumentState {
    crate::support::roundtrip(s.snapshot()).document().clone()
}

fn frame(s: &AuthoringSession, p: f32, b: f32) -> DrawableFrame {
    crate::support::frame(
        s.snapshot(),
        &s.snapshot()
            .document()
            .parameter_order()
            .iter()
            .map(|parameter| {
                (
                    parameter.clone(),
                    if parameter == PARAMETER_ID {
                        p
                    } else if parameter == &id(1700) {
                        b
                    } else {
                        0.
                    },
                )
            })
            .collect(),
    )
    .unwrap()
}

fn shared() -> AuthoringSession {
    let mut s = base();
    let p = id(1700);
    let table = id(1701);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: p.clone(),
                        kind: ParameterKind::BlendShape,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("blend parameter")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    apply(
        &mut s,
        E::Blend(B::CreateTable {
            table: BlendShapeKeyTable {
                id: table.clone(),
                parameter_id: p,
                keys: vec![-1., 0., 1.],
                base_key_idx: 1,
                samples: vec![],
                base_sample_id: String::new(),
            },
        }),
    );
    for n in [1702, 1703] {
        let b = BlendShapeBinding {
            id: id(n),
            target_id: MESH_ID.into(),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: table.clone(),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Mesh(
                [-4., 0., 8.]
                    .into_iter()
                    .map(|x| DeltaMeshKeyform {
                        positions: vec![Vec2::new(x, 0.); 4],
                        ..Default::default()
                    })
                    .collect(),
            ),
        };
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    let binding = b;
                    e.edit_blend_shape(
                        zipzi_sdk::BlendAuthoringEdit::CreateBinding {
                            id: binding.id.clone(),
                            target_id: binding.target_id.clone(),
                            target_kind: binding.target_kind,
                            key_table_id: binding.key_table_id.clone(),
                        },
                        Default::default(),
                        &Default::default(),
                    )?;
                    e.replace_blend_binding(binding)
                })
                .and_then(|value| {
                    let prepared = draft.prepare("shared user")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
    }
    s
}

mod identity;
mod offscreen_blend;
mod rotation;
mod shared_rebase;
mod shared_tables;
mod shared_validation;
mod target_grids;
mod topology;
