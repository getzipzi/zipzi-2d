//! Native acceptance uses the same library as the application.
fn main() -> anyhow::Result<()> {
    kasane_editor::acceptance::timeline()
}
