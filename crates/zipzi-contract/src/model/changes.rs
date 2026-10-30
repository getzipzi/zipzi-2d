#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChangeKind {
    #[default]
    None,
    Metadata,
    /// Evaluated appearance changes without altering topology or dependencies.
    Appearance,
    Resources,
    Positions,
    Structure,
}
