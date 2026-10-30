use super::*;
use planning::{
    ObservationContentIdentity, ObservationTexturePlan, ObservationTextureSize,
    ObservationTextureTarget,
};

pub(crate) const AREA_FILTER_VERSION: u32 = 1;

impl ResolvedObservation {
    pub(super) fn check_preparation_workspace(
        target: &ObservationTextureTarget,
        limits: ObservationCaptureLimits,
        source_alpha: bool,
        source_is_shared: bool,
    ) -> Result<(), ObservationError> {
        let source = target.source.source_size;
        let render = target.render_size;
        let source_bytes = u64::from(source.width)
            .checked_mul(u64::from(source.height))
            .and_then(|n| n.checked_mul(4));
        let render_bytes = u64::from(render.width)
            .checked_mul(u64::from(render.height))
            .and_then(|n| n.checked_mul(4));
        // Source Arc plus target Vec->Arc conversion, optional source samples,
        // and small hashing/loop state. An unchanged base reuses its source Arc.
        let alpha_step = source.width.max(source.height).div_ceil(256).max(1);
        let alpha_bytes = u64::from(source.width.div_ceil(alpha_step))
            * u64::from(source.height.div_ceil(alpha_step));
        let required = (if source_is_shared {
            Some(0)
        } else {
            source_bytes
        })
        .and_then(|n| {
            n.checked_add(if source != render {
                render_bytes?.checked_mul(2)?
            } else {
                0
            })
        })
        .and_then(|n| n.checked_add(if source_alpha { alpha_bytes * 2 } else { 0 }))
        .and_then(|n| n.checked_add(4096));
        if required.is_none_or(|n| n > limits.png.max_workspace_bytes as u64) {
            return Err(ObservationError {
                code: "DECODE_WORKSPACE_LIMIT".into(),
                message: format!(
                    "Texture preparation requires {required:?} bytes, limit {}",
                    limits.png.max_workspace_bytes
                ),
                asset_id: Some(target.source.asset_id.clone()),
            });
        }
        Ok(())
    }

    pub(super) fn prepare_texture(
        asset: ImageAsset,
        target: &ObservationTextureTarget,
        verified: zipzi_project::VerifiedTexture,
        save_alpha: bool,
        control: &zipzi_sdk::ConversionControl,
    ) -> Result<ResolvedTexture, ObservationError> {
        let mut data = verified.texture;
        let size = ObservationTextureSize {
            width: data.width,
            height: data.height,
        };
        if size != target.source.source_size {
            return Err(ObservationError {
                code: "RESOURCE_DIMENSIONS".into(),
                message: "Verified pixels differ from frozen texture dimensions".into(),
                asset_id: Some(asset.id),
            });
        }
        let source_alpha = save_alpha
            .then(|| {
                alpha::SourceAlphaSamples::extract(data.width, data.height, &data.rgba, control)
            })
            .transpose()?;
        if target.render_size != size {
            #[cfg(feature = "gpu")]
            {
                let pixels = zipzi_render::texture::area_average_rgba8_with_control(
                    data.width,
                    data.height,
                    &data.rgba,
                    target.render_size.width,
                    target.render_size.height,
                    || !control.is_cancelled(),
                )
                .ok_or_else(|| ObservationError {
                    code: "CANCELLED".into(),
                    message: "Texture downsampling cancelled".into(),
                    asset_id: Some(asset.id.clone()),
                })?;
                data = TextureData {
                    rgba: pixels.into(),
                    width: target.render_size.width,
                    height: target.render_size.height,
                    cache_key: String::new(),
                };
            }
            #[cfg(not(feature = "gpu"))]
            return Err(ObservationError {
                code: "GPU_UNAVAILABLE".into(),
                message: "Texture downsampling requires the observation gpu feature".into(),
                asset_id: Some(asset.id),
            });
        }
        let render_rgba_hash = if target.render_size == size {
            match &verified.source_content_identity {
                ObservationContentIdentity::Rgba8Sha256 { hash } => hash.clone(),
                _ => zipzi_project::resources::rgba8_sha256_with_control(
                    data.width,
                    data.height,
                    &data.rgba,
                    || !control.is_cancelled(),
                )
                .map_err(|s| ObservationError {
                    code: s.code,
                    message: s.message,
                    asset_id: Some(asset.id.clone()),
                })?,
            }
        } else {
            zipzi_project::resources::rgba8_sha256_with_control(
                data.width,
                data.height,
                &data.rgba,
                || !control.is_cancelled(),
            )
            .map_err(|s| ObservationError {
                code: s.code,
                message: s.message,
                asset_id: Some(asset.id.clone()),
            })?
        };
        data.cache_key = format!("rgba8-v1:{render_rgba_hash}");
        check_capture(control)?;
        Ok(ResolvedTexture {
            asset,
            data,
            source_content_identity: verified.source_content_identity,
            render_rgba_hash,
            source_alpha,
        })
    }

    pub(super) fn report(
        plan: &ObservationTexturePlan,
        textures: &[ResolvedTexture],
    ) -> ObservationTextureReport {
        ObservationTextureReport {
            policy: plan.policy.clone(),
            effective_max_rgba_bytes: plan.effective_max_rgba_bytes,
            longest_edge_limit: plan.longest_edge_limit,
            source_rgba_bytes: plan.source_rgba_bytes,
            render_rgba_bytes: plan.render_rgba_bytes,
            estimated_mipmap_bytes: plan.estimated_mipmap_bytes,
            resized_count: plan.resized_count,
            filter: "alpha_weighted_area_average".into(),
            algorithm_version: AREA_FILTER_VERSION,
            planning_version: plan.planning_version,
            mipmap_version: plan.mipmap_version,
            textures: textures
                .iter()
                .map(|t| ObservationTextureReportEntry {
                    asset_id: t.asset.id.clone(),
                    source_size: ObservationTextureSize {
                        width: t.asset.content.as_ref().unwrap().width,
                        height: t.asset.content.as_ref().unwrap().height,
                    },
                    render_size: ObservationTextureSize {
                        width: t.data.width,
                        height: t.data.height,
                    },
                    source_content_identity: t.source_content_identity.clone(),
                    render_rgba_hash: t.render_rgba_hash.clone(),
                })
                .collect(),
        }
    }
}

#[cfg(all(test, feature = "gpu"))]
mod tests;
