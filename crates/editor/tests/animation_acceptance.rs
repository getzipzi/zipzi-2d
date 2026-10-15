//! Native acceptance uses the same library as the application.
fn main() -> anyhow::Result<()> {
    kasane_editor::acceptance::animation(
        std::env::args()
            .skip(1)
            .filter(|arg| !arg.starts_with('-'))
            .map(Into::into)
            .collect(),
    )
}
