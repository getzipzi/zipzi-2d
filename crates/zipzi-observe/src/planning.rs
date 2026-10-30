//! Metadata-only observation texture planning. No image I/O, pose evaluation,
//! decoder or GPU initialization is needed. Execution must validate sources.

use std::collections::{BTreeMap, BTreeSet};

pub use zipzi_contract::runtime::{
    ObservationContentIdentity, ObservationTexturePlan, ObservationTexturePlanning,
    ObservationTexturePolicy, ObservationTextureSize, ObservationTextureSource,
    ObservationTextureTarget, DEFAULT_OBSERVATION_TEXTURE_RGBA_BYTES,
};
use zipzi_core::{Document, DrawableFrame, ImageAsset};

use crate::ObservationError;

pub const PLANNING_VERSION: u32 = 1;
pub const MIPMAP_VERSION: u32 = 2;

fn invalid(code: &str, message: impl Into<String>, asset_id: Option<&str>) -> ObservationError {
    ObservationError {
        code: code.into(),
        message: message.into(),
        asset_id: asset_id.map(str::to_owned),
    }
}

/// All mesh references, including disabled/hidden meshes and all mask sources.
/// Mesh and Offscreen masks name meshes in this same collection, not assets.
/// ROI, poses, visibility and encoded content do not affect the set.
pub fn referenced_assets(document: &Document) -> Result<Vec<ImageAsset>, ObservationError> {
    let mut assets = BTreeMap::new();
    for mesh_id in document.mesh_order() {
        let mesh = document.get_mesh(mesh_id).expect("document mesh order");
        let id = mesh.texture_asset_id.resolved_id().ok_or_else(|| {
            invalid(
                "MISSING_ASSET",
                format!("Mesh {mesh_id} has no resolved texture reference"),
                None,
            )
        })?;
        if assets.contains_key(id) {
            continue;
        }
        let asset = document
            .get_asset(id)
            .ok_or_else(|| invalid("MISSING_ASSET", "Mesh references an absent asset", Some(id)))?;
        assets.insert(id.to_owned(), asset.clone());
    }
    Ok(assets.into_values().collect())
}

/// Reject a draw or mask texture absent from the frozen metadata reference set.
/// Never silently resolve additional, uncharged textures during execution.
pub(crate) fn validate_frame_references(
    frame: &DrawableFrame,
    assets: &[ImageAsset],
) -> Result<(), ObservationError> {
    let required: BTreeSet<_> = assets.iter().map(|a| a.id.as_str()).collect();
    for drawable in &frame.drawables {
        if !required.contains(drawable.texture_asset_id.as_str()) {
            return Err(invalid(
                "UNPLANNED_TEXTURE",
                "Evaluated drawable texture is absent from the source reference set",
                Some(&drawable.texture_asset_id),
            ));
        }
    }
    Ok(())
}

/// Collect sources without reading files. PNG identities here are declarations,
/// not verified identities; generated memory sources can have no declaration.
pub fn texture_sources(
    document: &Document,
) -> Result<Vec<ObservationTextureSource>, ObservationError> {
    sources_from_assets(&referenced_assets(document)?)
}

pub(crate) fn sources_from_assets(
    assets: &[ImageAsset],
) -> Result<Vec<ObservationTextureSource>, ObservationError> {
    assets
        .iter()
        .map(|asset| {
            let content = asset
                .image_content()
                .map_err(|e| invalid(&e.code, e.message, Some(&asset.id)))?;
            Ok(ObservationTextureSource {
                asset_id: asset.id.clone(),
                source_size: ObservationTextureSize {
                    width: content.width,
                    height: content.height,
                },
                source_content_identity: (!content.sha256.is_empty()).then(|| {
                    ObservationContentIdentity::PngSha256 {
                        hash: content.sha256.clone(),
                    }
                }),
            })
        })
        .collect()
}

fn rgba_bytes(size: ObservationTextureSize, id: Option<&str>) -> Result<u64, ObservationError> {
    if size.width == 0 || size.height == 0 {
        return Err(invalid(
            "RESOURCE_DIMENSIONS",
            "Texture axes must be positive",
            id,
        ));
    }
    u64::from(size.width)
        .checked_mul(u64::from(size.height))
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| invalid("RESOURCE_DIMENSIONS", "Texture RGBA size overflows u64", id))
}

/// Non-base logical RGBA bytes, computed level by level for odd and thin images.
pub fn estimated_mipmap_bytes(mut size: ObservationTextureSize) -> Result<u64, ObservationError> {
    rgba_bytes(size, None)?;
    let mut total = 0u64;
    while size.width > 1 || size.height > 1 {
        size.width = (size.width / 2).max(1);
        size.height = (size.height / 2).max(1);
        total = total
            .checked_add(rgba_bytes(size, None)?)
            .ok_or_else(|| invalid("RESOURCE_DIMENSIONS", "Mipmap byte sum overflows u64", None))?;
    }
    Ok(total)
}

fn scaled(size: ObservationTextureSize, limit: u32) -> ObservationTextureSize {
    let longest = size.width.max(size.height);
    if longest <= limit {
        return size;
    }
    // Products of two u32 values fit in u64. Integer division is intentional.
    ObservationTextureSize {
        width: (u64::from(size.width) * u64::from(limit) / u64::from(longest)).max(1) as u32,
        height: (u64::from(size.height) * u64::from(limit) / u64::from(longest)).max(1) as u32,
    }
}

/// Produce a deterministic plan or a budget diagnostic. Invalid policies and
/// malformed/overflowing metadata are errors even for an empty source set.
pub fn plan_textures(
    sources: impl IntoIterator<Item = ObservationTextureSource>,
    policy: ObservationTexturePolicy,
    host_max_rgba_bytes: u64,
) -> Result<ObservationTexturePlanning, ObservationError> {
    let budget = match &policy {
        ObservationTexturePolicy::Original {} => host_max_rgba_bytes,
        ObservationTexturePolicy::FitBudget { max_rgba_bytes }
            if *max_rgba_bytes > 0 && *max_rgba_bytes <= host_max_rgba_bytes =>
        {
            *max_rgba_bytes
        }
        ObservationTexturePolicy::FitBudget { max_rgba_bytes } => {
            return Err(invalid(
                "INVALID_TEXTURE_POLICY",
                format!("Texture budget {max_rgba_bytes} must be in 1..={host_max_rgba_bytes}"),
                None,
            ))
        }
    };
    let mut unique = BTreeMap::new();
    for source in sources {
        rgba_bytes(source.source_size, Some(&source.asset_id))?;
        if source.asset_id.is_empty() {
            return Err(invalid(
                "INVALID_ASSET",
                "Texture asset ID must be nonempty",
                None,
            ));
        }
        if let Some(previous) = unique.get(&source.asset_id) {
            if previous != &source {
                return Err(invalid(
                    "MIXED_DOCUMENT_SNAPSHOTS",
                    "Conflicting texture metadata",
                    Some(&source.asset_id),
                ));
            }
        } else {
            unique.insert(source.asset_id.clone(), source);
        }
    }
    let sources: Vec<_> = unique.into_values().collect();
    let source_bytes = sources.iter().try_fold(0u64, |total, source| {
        total
            .checked_add(rgba_bytes(source.source_size, Some(&source.asset_id))?)
            .ok_or_else(|| {
                invalid(
                    "RESOURCE_DIMENSIONS",
                    "Source RGBA sum overflows u64",
                    Some(&source.asset_id),
                )
            })
    })?;
    let maximum_edge = sources
        .iter()
        .map(|s| s.source_size.width.max(s.source_size.height))
        .max();
    let minimum = match policy {
        ObservationTexturePolicy::Original {} => source_bytes,
        ObservationTexturePolicy::FitBudget { .. } => u64::try_from(sources.len())
            .ok()
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| invalid("RESOURCE_DIMENSIONS", "Texture count overflows u64", None))?,
    };
    if minimum > budget {
        return Ok(ObservationTexturePlanning::InsufficientBudget {
            policy,
            effective_max_rgba_bytes: budget,
            source_rgba_bytes: source_bytes,
            minimum_required_rgba_bytes: minimum,
            textures: sources,
        });
    }
    let limit = if source_bytes <= budget {
        maximum_edge
    } else {
        let (mut low, mut high) = (1, maximum_edge.expect("nonempty over-budget set"));
        while low < high {
            let middle = low + (high - low).div_ceil(2);
            let total = sources.iter().try_fold(0u64, |total, source| {
                total
                    .checked_add(rgba_bytes(
                        scaled(source.source_size, middle),
                        Some(&source.asset_id),
                    )?)
                    .ok_or_else(|| {
                        invalid(
                            "RESOURCE_DIMENSIONS",
                            "Target RGBA sum overflows u64",
                            Some(&source.asset_id),
                        )
                    })
            })?;
            if total <= budget {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        Some(low)
    };
    let mut plan = ObservationTexturePlan {
        policy,
        effective_max_rgba_bytes: budget,
        longest_edge_limit: limit,
        source_rgba_bytes: source_bytes,
        render_rgba_bytes: 0,
        estimated_mipmap_bytes: 0,
        resized_count: 0,
        planning_version: PLANNING_VERSION,
        mipmap_version: MIPMAP_VERSION,
        textures: Vec::with_capacity(sources.len()),
    };
    for source in sources {
        let render_size = scaled(source.source_size, limit.expect("nonempty set"));
        plan.render_rgba_bytes = plan
            .render_rgba_bytes
            .checked_add(rgba_bytes(render_size, Some(&source.asset_id))?)
            .ok_or_else(|| invalid("RESOURCE_DIMENSIONS", "Target RGBA sum overflows u64", None))?;
        plan.estimated_mipmap_bytes = plan
            .estimated_mipmap_bytes
            .checked_add(estimated_mipmap_bytes(render_size)?)
            .ok_or_else(|| invalid("RESOURCE_DIMENSIONS", "Mipmap byte sum overflows u64", None))?;
        plan.resized_count += usize::from(source.source_size != render_size);
        plan.textures.push(ObservationTextureTarget {
            source,
            render_size,
        });
    }
    Ok(ObservationTexturePlanning::Ready { plan })
}

#[cfg(test)]
mod tests;
