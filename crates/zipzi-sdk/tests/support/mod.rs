#![allow(dead_code)]
use zipzi_core::document::{SceneCreate, SceneEdit};
use zipzi_sdk::{AuthoringSession, Commands, PreparedChange, Snapshot, Status};

/// Assemble a private candidate using the public authoring boundary.
pub fn prepared<T>(
    session: &AuthoringSession,
    write: impl FnOnce(&mut Commands<'_>) -> Result<T, Status>,
) -> Result<(T, PreparedChange), Status> {
    let mut draft = session.begin_draft();
    let outcome = draft.batch(draft.source(), write)?;
    Ok((outcome, draft.prepare("Test edit")?))
}

mod records;
pub use records::*;

pub fn roundtrip(snapshot: &Snapshot) -> Snapshot {
    let root = tempfile::tempdir().unwrap();
    let io = zipzi_project::ProjectIo::new(root.path(), Default::default());
    let path = root.path().join("native.zipzi");
    io.save(
        snapshot.document(),
        snapshot.resources(),
        &path,
        None,
        || true,
    )
    .unwrap();
    let loaded = io.load(&path, || true).unwrap();
    Snapshot::new(loaded.document, loaded.resources).unwrap()
}

#[path = "../../../zipzi-project/tests/support/mod.rs"]
pub mod archive;
pub fn frame(
    snapshot: &Snapshot,
    values: &zipzi_core::PreviewValues,
) -> Result<zipzi_core::DrawableFrame, Status> {
    let mut frame = zipzi_core::DrawableFrame::default();
    let status = zipzi_core::evaluation::evaluate_frame(snapshot.document(), values, &mut frame);
    if status.is_ok() {
        Ok(frame)
    } else {
        Err(status)
    }
}

pub fn binding(
    s: &AuthoringSession,
    edit: zipzi_contract::authoring::BindingAuthoringEdit,
) -> Result<
    (
        zipzi_core::state::ConversionOutcome,
        zipzi_sdk::PreparedChange,
    ),
    Status,
> {
    binding_with_control(s, edit, Default::default(), &Default::default())
}
pub fn binding_with_control(
    s: &AuthoringSession,
    edit: zipzi_contract::authoring::BindingAuthoringEdit,
    limits: zipzi_sdk::ConversionLimits,
    control: &zipzi_sdk::ConversionControl,
) -> Result<
    (
        zipzi_core::state::ConversionOutcome,
        zipzi_sdk::PreparedChange,
    ),
    Status,
> {
    use zipzi_contract::authoring::BindingAuthoringEdit as E;
    prepared(s, |c| match edit {
        E::Grid(e) => c.edit_binding_grid(e, limits, control),
        E::Blend(e) => c.edit_blend_shape(e, limits, control),
        E::ParameterRange(e) => c.edit_parameter_range(e, limits, control),
        E::Reparent(edit) => Ok(c
            .edit_scene(
                zipzi_core::document::SceneEdit::Reparent { edit },
                limits,
                control,
            )?
            .conversion
            .expect("reparent conversion")),
    })
}
pub fn axis(s: &AuthoringSession, owner: &str, parameter: &str) -> String {
    s.snapshot()
        .document()
        .binding(owner)
        .unwrap()
        .axes
        .iter()
        .find(|a| a.parameter_id == parameter)
        .unwrap()
        .id
        .clone()
}
pub fn sample(s: &AuthoringSession, owner: &str, parameter: &str, value: f32) -> String {
    s.snapshot()
        .document()
        .binding(owner)
        .unwrap()
        .axes
        .iter()
        .find(|a| a.parameter_id == parameter)
        .unwrap()
        .samples
        .iter()
        .find(|x| x.value == value)
        .unwrap()
        .id
        .clone()
}

pub fn insert_mesh(c: &mut Commands<'_>, mesh: zipzi_core::Mesh) -> Result<(), Status> {
    c.edit_scene(
        SceneEdit::Create {
            id: mesh.id.clone(),
            name: mesh.name.clone(),
            object: SceneCreate::Mesh {
                texture: mesh.texture_asset_id.clone(),
            },
            part_id: (!mesh.part_id.is_empty()).then(|| mesh.part_id.clone()),
            parent_id: (!mesh.deformer_id.is_empty()).then(|| mesh.deformer_id.clone()),
            before: None,
            deformation_before: None,
        },
        Default::default(),
        &Default::default(),
    )?;
    c.replace_mesh(mesh)
}

/// Assemble drawing fixtures with the same public operations used by clients.
pub fn drawing_groups(
    c: &mut Commands<'_>,
    groups: Vec<zipzi_core::draw_order::DrawOrderGroup>,
) -> Result<(), Status> {
    let current = c.read_document()?;
    for group in zipzi_core::draw_order::resolved_groups(&current)
        .into_iter()
        .rev()
    {
        if !group.owner.is_empty() && !groups.iter().any(|g| g.owner == group.owner) {
            c.edit_scene(
                SceneEdit::DissolveDrawingGroup { owner: group.owner },
                Default::default(),
                &Default::default(),
            )?;
        }
    }
    for group in &groups {
        if !group.owner.is_empty()
            && c.read_document()?
                .drawing_group_range(&group.owner)
                .is_err()
        {
            let parent = groups
                .iter()
                .find(|g| g.items.contains(&group.owner))
                .map(|g| g.owner.clone())
                .unwrap_or_default();
            c.edit_scene(
                SceneEdit::EnsureDrawingGroup {
                    owner: group.owner.clone(),
                    parent,
                    before: None,
                },
                Default::default(),
                &Default::default(),
            )?;
        }
    }
    for group in groups.iter().rev() {
        if !group.items.is_empty() {
            for id in &group.items {
                c.edit_scene(
                    SceneEdit::Move {
                        ids: vec![id.clone()],
                        destination: zipzi_core::document::SceneCollection::Drawing {
                            owner: group.owner.clone(),
                        },
                        before: None,
                    },
                    Default::default(),
                    &Default::default(),
                )?;
            }
        }
        let expected = c.read_document()?.drawing_group_range(&group.owner)?;
        c.edit_scene(
            SceneEdit::SetDrawingRange {
                owner: group.owner.clone(),
                expected,
                range: [group.min_order, group.max_order],
            },
            Default::default(),
            &Default::default(),
        )?;
    }
    Ok(())
}
