//! Two dedicated images and independently identified meshes with colliding vertex IDs.
use zipzi_core::{Canvas, Vec2};
use zipzi_sdk::{AuthoringSession, ResourceStore, Snapshot, rectangle_mesh};
pub fn id(n: u128) -> String {
    uuid::Uuid::from_u128(9400 + n).to_string()
}
pub fn create(path: &std::path::Path, png: &[u8]) -> anyhow::Result<AuthoringSession> {
    let image = path.join("multi.png");
    std::fs::write(&image, png)?;
    let state = zipzi_core::state::DocumentState::new(
        &id(0),
        Canvas::new(256., 256., Vec2::default(), 1.).into(),
    )?;
    let pixels = ResourceStore::default().import_image(&image)?;
    let mut session = AuthoringSession::new(Snapshot::new(state, Default::default())?);
    let mut draft = session.begin_draft();
    draft.batch(draft.source(), |edit| {
        for (asset, mesh, x, y, width, height) in
            [(1, 2, 20., 20., 80., 80.), (3, 4, 140., 140., 40., 80.)]
        {
            edit.create_empty_asset(id(asset), "Image".into())?;
            edit.replace_image(&id(asset), pixels.clone())?;
            edit.create_mesh(rectangle_mesh(
                &id(mesh),
                &format!("Mesh {mesh}"),
                &id(asset),
                Vec2::new(x, y),
                Vec2::new(x + width, y + height),
            )?)?;
        }
        Ok(())
    })?;
    session.publish(draft.prepare("Fixture")?)?;
    Ok(session)
}
