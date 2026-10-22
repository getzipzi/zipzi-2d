use super::*;
use kasane_core::*;
use kasane_sdk::{
    AxisReduction as R, BindingAuthoringEdit as E, BindingFieldPatch, BindingGridEdit as G,
    BindingGridTarget as T, BlendAuthoringEdit as B, BlendTableOperation as BO, GridOperation as O,
    ObjectKind,
};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn apply(s: &mut AuthoringSession, e: E) -> kasane_sdk::EditReceipt {
    let p = s.plan_binding_authoring(e).unwrap();
    s.execute_binding_plan(&p).unwrap()
}

fn base() -> AuthoringSession {
    let mut s = fixture();
    let b = binding(&s);
    s.edit("parameters", None, |e| {
        e.create_parameter(parameter(false))?;
        e.create_parameter(Parameter {
            id: id(1600),
            ..Default::default()
        })?;
        e.create_binding(b)
    })
    .unwrap();
    s
}

fn op(s: &mut AuthoringSession, owner: &str, operation: O) -> kasane_sdk::EditReceipt {
    apply(
        s,
        E::Grid(G::Edit {
            owner: owner.into(),
            operation,
        }),
    )
}

fn roundtrip(s: &AuthoringSession) -> Document {
    let mut d = s.document().fork_candidate();
    let mut a = d.get_asset(ASSET_ID).unwrap().clone();
    a.content.as_mut().unwrap().source = "assets/texture.png".into();
    assert!(d.replace_asset(a).status.is_ok());
    let text = kasane_project::encode_project(&d).unwrap();
    let out = kasane_project::decode_project(&text).unwrap();
    assert!(
        out.validate_structure().is_empty(),
        "{:?}",
        out.validate_structure()
    );
    out
}

fn frame(s: &AuthoringSession, p: f32, b: f32) -> DrawableFrame {
    s.evaluate(
        &s.parameter_ids()
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
    s.edit("blend parameter", None, |e| {
        e.create_parameter(Parameter {
            id: p.clone(),
            kind: ParameterKind::BlendShape,
            ..Default::default()
        })
    })
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
        s.edit("shared user", None, |e| e.create_blend_binding(b))
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
