pub mod deformer_tree;
mod hierarchy_tree;
pub mod mesh_tree;
pub mod parameters;
pub mod properties;
mod sorting;
pub(crate) mod toolbar;

pub use deformer_tree::DeformerTreePanel;
pub use mesh_tree::MeshTreePanel;
pub use parameters::ParametersPanel;
pub use properties::PropertiesPanel;

pub mod document_panel;
pub use document_panel::{DocumentPanel, DocumentPanelKind};

pub(crate) mod animation_feedback;
pub mod animation_panel;
