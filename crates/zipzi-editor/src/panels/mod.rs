pub mod deformer_tree;
mod group_header;
mod hierarchy_tree;
pub mod parameters;
pub mod part_tree;
pub mod properties;
mod sorting;
pub(crate) use sorting::open_resource_destination;
pub mod timeline;
pub(crate) mod toolbar;

pub use deformer_tree::DeformerTreePanel;
pub use parameters::ParametersPanel;
pub use part_tree::PartsTreePanel;
pub use properties::PropertiesPanel;

pub mod document_panel;
pub use document_panel::DocumentPanel;

pub(crate) mod animation_feedback;
pub mod animation_panel;
