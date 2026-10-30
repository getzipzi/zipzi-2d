//! Shared model values.

use serde::{Deserialize, Serialize};

// Include the domain files in this module to retain type paths and wire signatures.
include!("model/basics.rs");
include!("model/appearance.rs");
include!("model/parameters_transforms.rs");
include!("model/scene_keyforms_meshes.rs");
include!("model/blendshapes.rs");
include!("model/glue_bindings.rs");
include!("model/offscreens_scene_bindings.rs");
include!("model/changes.rs");
