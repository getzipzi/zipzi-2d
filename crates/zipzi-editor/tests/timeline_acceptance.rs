//! Native acceptance uses the same library as the application.
fn main() -> anyhow::Result<()> {
    zipzi_editor::acceptance::timeline()
}
