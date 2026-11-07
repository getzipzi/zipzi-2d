use zipzi_core::document::{SceneCreate, SceneEdit};
use zipzi_core::{Glue, Offscreen, Transform, TransformData};
use zipzi_sdk::{Commands, Status};

pub struct CapturedImage {
    pub id: String,
    pub name: String,
    pub image: zipzi_project::ImageResource,
}
pub fn image_fixture(
    id: &str,
    name: &str,
    path: &std::path::Path,
) -> Result<CapturedImage, Status> {
    let image = zipzi_project::ResourceStore::default()
        .import_image(path)
        .map_err(Status::from)?;
    Ok(CapturedImage {
        id: id.into(),
        name: name.into(),
        image,
    })
}
pub fn insert_image(c: &mut Commands<'_>, asset: CapturedImage) -> Result<(), Status> {
    c.create_empty_asset(asset.id.clone(), asset.name)?;
    c.replace_image(&asset.id, asset.image)
}
pub fn insert_transform(c: &mut Commands<'_>, mut transform: Transform) -> Result<(), Status> {
    let object = match transform.data {
        TransformData::Warp(_) => SceneCreate::Warp { bounds: None },
        TransformData::Rotation(_) => SceneCreate::Rotation { origin: None },
    };
    c.edit_scene(
        SceneEdit::Create {
            id: transform.id.clone(),
            name: transform.name.clone(),
            object,
            part_id: transform.part_id.as_ref().map(|id| id.as_str().to_owned()),
            parent_id: transform
                .parent_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            before: None,
            deformation_before: None,
        },
        Default::default(),
        &Default::default(),
    )?;
    if transform.runtime_id.is_empty() {
        transform.runtime_id = transform.id.clone();
    }
    c.replace_transform(transform)
}
pub fn insert_offscreen(c: &mut Commands<'_>, mut offscreen: Offscreen) -> Result<(), Status> {
    c.edit_scene(
        SceneEdit::Create {
            id: offscreen.id.clone(),
            name: offscreen.name.clone(),
            object: SceneCreate::Offscreen,
            part_id: Some(offscreen.part_id.clone()),
            parent_id: None,
            before: None,
            deformation_before: None,
        },
        Default::default(),
        &Default::default(),
    )?;
    if offscreen.runtime_id.is_empty() {
        offscreen.runtime_id = offscreen.id.clone();
    }
    c.replace_offscreen(offscreen)
}
pub fn insert_glue(c: &mut Commands<'_>, mut glue: Glue) -> Result<(), Status> {
    c.edit_scene(
        SceneEdit::Create {
            id: glue.id.clone(),
            name: glue.name.clone(),
            object: SceneCreate::Glue {
                a: glue.mesh_a_id.clone(),
                b: glue.mesh_b_id.clone(),
            },
            part_id: None,
            parent_id: None,
            before: None,
            deformation_before: None,
        },
        Default::default(),
        &Default::default(),
    )?;
    if glue.runtime_id.is_empty() {
        glue.runtime_id = glue.id.clone();
    }
    c.replace_glue(glue)
}
