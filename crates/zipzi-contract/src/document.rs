//! Persistent document values.

use crate::model::*;
use crate::scene_reference::SceneReference;
use crate::warp_bezier::WarpBezier;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

// Include the domain files in this module to retain type paths and wire signatures.
include!("document/state_and_pose.rs");
include!("document/motion.rs");
include!("document/display_info.rs");
include!("document/pose_motion_edits.rs");
include!("document/resource_edits.rs");
include!("document/model_references.rs");
include!("document/organization.rs");
include!("document/scene.rs");
include!("document/scene_content.rs");
include!("document/binding_grids.rs");
include!("document/binding_reparent.rs");
include!("document/deletion_requests.rs");
include!("document/deletion_outcomes.rs");
include!("document/scene_content_values.rs");
