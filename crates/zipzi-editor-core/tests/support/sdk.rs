#![allow(dead_code)]
use zipzi_core::document::{
    OrganizationCollection, OrganizationEdit, ReparentEdit, ReparentStrategy, SceneEdit,
};
use zipzi_core::{BlendShapeBinding, BlendShapeKeyTable, state::DocumentState};
use zipzi_sdk::{
    AuthoringSession, BlendAuthoringEdit, Commands, ProjectIo, Publication, SaveReceipt, Snapshot,
    Status,
};

#[path = "../../../zipzi-sdk/tests/support/records.rs"]
mod sdk_fixtures;
#[allow(unused_imports)]
pub use sdk_fixtures::{
    image_fixture, insert_glue, insert_image, insert_offscreen, insert_transform,
};

pub fn fixture_session(
    id: &str,
    setup: impl Into<zipzi_core::ModelSetup>,
) -> Result<AuthoringSession, Status> {
    let document = DocumentState::new(id, setup.into())?;
    Ok(AuthoringSession::new(Snapshot::new(
        document,
        Default::default(),
    )?))
}

fn project_io(path: &std::path::Path) -> ProjectIo {
    ProjectIo::new(
        path.parent().unwrap().join("zipzi-fixture-cache"),
        Default::default(),
    )
}

pub fn open_fixture(path: &std::path::Path) -> Result<AuthoringSession, Status> {
    AuthoringSession::open(&project_io(path), path, || true).map(|(session, _)| session)
}

pub fn save_fixture(
    session: &mut AuthoringSession,
    path: &std::path::Path,
) -> Result<SaveReceipt, Status> {
    session.save_as(&project_io(path), path, None, || true)
}

pub fn publish_fixture(
    session: &mut AuthoringSession,
    description: &str,
    write: impl FnOnce(&mut Commands<'_>) -> Result<(), Status>,
) -> Result<Publication, Status> {
    let mut draft = session.begin_draft();
    draft.batch(draft.source(), write)?;
    session.publish(draft.prepare(description)?)
}

pub fn organize_mesh(
    commands: &mut Commands<'_>,
    mesh_id: &str,
    part_id: &str,
) -> Result<(), Status> {
    commands
        .edit_organization(OrganizationEdit::Move {
            ids: vec![mesh_id.into()],
            destination: OrganizationCollection::PartMembers {
                part_id: Some(part_id.into()),
            },
            before: None,
        })
        .map(|_| ())
}

pub fn reparent_local(
    commands: &mut Commands<'_>,
    id: &str,
    parent_id: &str,
) -> Result<(), Status> {
    commands
        .edit_scene(
            SceneEdit::Reparent {
                edit: ReparentEdit {
                    sources: vec![id.into()],
                    parent_id: parent_id.into(),
                    strategy: ReparentStrategy::KeepLocalData,
                    placement: None,
                    tolerance: 1e-6,
                    relative_tolerance: 1e-6,
                    context: None,
                },
            },
            Default::default(),
            &Default::default(),
        )
        .map(|_| ())
}

pub fn insert_blend_table(
    commands: &mut Commands<'_>,
    table: BlendShapeKeyTable,
) -> Result<(), Status> {
    commands
        .edit_blend_shape(
            BlendAuthoringEdit::CreateTable { table },
            Default::default(),
            &Default::default(),
        )
        .map(|_| ())
}

pub fn insert_blend_binding(
    commands: &mut Commands<'_>,
    binding: BlendShapeBinding,
) -> Result<(), Status> {
    commands.edit_blend_shape(
        BlendAuthoringEdit::CreateBinding {
            id: binding.id.clone(),
            target_id: binding.target_id.clone(),
            target_kind: binding.target_kind,
            key_table_id: binding.key_table_id.clone(),
        },
        Default::default(),
        &Default::default(),
    )?;
    commands.replace_blend_binding(binding)
}

pub fn insert_fixture_mesh(
    commands: &mut Commands<'_>,
    mut mesh: zipzi_core::Mesh,
) -> Result<(), Status> {
    if mesh.runtime_id.is_empty() {
        mesh.runtime_id = mesh.id.clone();
    }
    commands.create_mesh(mesh)
}

pub fn insert_fixture_part(
    commands: &mut Commands<'_>,
    mut part: zipzi_core::Part,
) -> Result<(), Status> {
    if part.runtime_id.is_empty() {
        part.runtime_id = part.id.clone();
    }
    commands.create_part(part)
}

pub fn insert_fixture_parameter(
    commands: &mut Commands<'_>,
    mut parameter: zipzi_core::Parameter,
) -> Result<(), Status> {
    if parameter.runtime_id.is_empty() {
        parameter.runtime_id = parameter.id.clone();
    }
    commands.create_parameter(parameter)
}
