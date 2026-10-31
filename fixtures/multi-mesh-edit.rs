//! Two dedicated images and independently identified meshes with colliding vertex IDs.
use zipzi_core::{Canvas, Vec2};
use zipzi_sdk::{AuthoringSession, prepare_png_asset, rectangle_mesh};
pub fn id(n: u128) -> String {
    uuid::Uuid::from_u128(9400 + n).to_string()
}
pub fn create(path: &std::path::Path, png: &[u8]) -> anyhow::Result<AuthoringSession> {
    let image = path.join("multi.png");
    std::fs::write(&image, png)?;
    let mut session = AuthoringSession::new(&id(0), Canvas::new(256., 256., Vec2::default(), 1.))?;
    session.edit("Fixture", None, |edit| {
        for (asset, mesh, x, y, width, height) in
            [(1, 2, 20., 20., 80., 80.), (3, 4, 140., 140., 40., 80.)]
        {
            edit.create_asset(prepare_png_asset(&id(asset), "Image", &image)?)?;
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
    Ok(session)
}
