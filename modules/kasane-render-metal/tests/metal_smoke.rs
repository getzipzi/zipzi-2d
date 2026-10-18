#![cfg(target_os = "macos")]

#[path = "metal_smoke/cropped_offscreens.rs"]
mod cropped_offscreens;
#[path = "metal_smoke/helpers.rs"]
mod helpers;
#[path = "metal_smoke/masks_atlas.rs"]
mod masks_atlas;
#[path = "metal_smoke/offscreens_blending.rs"]
mod offscreens_blending;
#[path = "metal_smoke/pipeline_textures.rs"]
mod pipeline_textures;
