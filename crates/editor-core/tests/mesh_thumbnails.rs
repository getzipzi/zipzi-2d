#[allow(dead_code)]
mod support;

#[test]
fn texture_regions_ignore_pose_appearance_and_names() -> anyhow::Result<()> {
    let (mut document, _assets) = support::sample_document_with_binding(true)?;
    let id = document.selected_id().unwrap().to_owned();
    let original = document.mesh_texture_region(&id).unwrap();
    let revision = document.mesh_texture_revision();
    let parameter = document.parameters()[0].id.clone();
    document.set_preview_parameter(&parameter, 1.)?;
    document.preview_opacity(0.25)?;
    assert_eq!(document.mesh_texture_revision(), revision);
    assert_eq!(document.mesh_texture_region(&id).unwrap(), original);
    document.finish_edit()?;
    assert_ne!(document.mesh_texture_revision(), revision);
    assert_eq!(document.mesh_texture_region(&id).unwrap(), original);
    document.preview_name("New name")?;
    document.finish_edit()?;
    document.set_visible(false)?;
    document.undo()?;
    document.redo()?;
    assert_eq!(document.mesh_texture_region(&id).unwrap(), original);
    assert!(document.mesh_texture_region("missing").is_none());
    Ok(())
}
