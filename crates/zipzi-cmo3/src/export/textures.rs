use super::*;

// Cubism pads GPU textures to multiples of the mipmap level without resizing
// the embedded PNG. Serialized UVs and the resource-to-logical transform must
// both account for that padding, just like CCachedImage does on initialization.
pub(super) fn texture_scale(w: u32, h: u32) -> Vec2 {
    Vec2::new(
        w as f32 / (w.div_ceil(64) * 64) as f32,
        h as f32 / (h.div_ceil(64) * 64) as f32,
    )
}

fn texture_transform(w: u32, h: u32) -> N {
    let scale = texture_scale(w, h);
    affine([scale.x as f64, 0., 0., 0., scale.y as f64, 0.])
}

impl Exporter<'_> {
    pub(super) fn cache(&mut self, resource: N, w: u32, h: u32) -> N {
        let cached = N::new("CCachedImage")
            .field("_cachedImageResource", resource.clone())
            .field("isSharedImage", b(true))
            .field(
                "rawImageSize",
                N::new("CSize").attr("width", w).attr("height", h),
            )
            .field("reductionRatio", i(1))
            .field("mipmapLevel", i(64))
            .field("hasMargin", b(false))
            .field("isCleaned", b(false))
            .field("transformRawImageToCachedImage", texture_transform(w, h));
        N::new("CCachedImageManager")
            .field(
                "defaultCacheType",
                N::enumeration("CachedImageType", "SCALE_1"),
            )
            .field("rawImage", resource)
            .field("cachedImages", N::list("array_list", [cached]))
            .field("requiredMipmapLevel", i(64))
    }
    pub(super) fn textures(
        &mut self,
        resolver: &dyn AssetResolver,
    ) -> Result<TextureSet, Cmo3Error> {
        let mut atlases = Vec::new();
        let mut textures = HashMap::new();
        for id in self.doc.asset_order() {
            let asset = self.doc.asset(id).unwrap();
            let zipzi_core::state::AssetContent::Image { width, height, .. } = asset.content else {
                return Err(Cmo3Error::Asset {
                    asset_id: id.clone(),
                    message: "Missing image content".into(),
                });
            };
            let bytes = resolver.read_png(asset)?;
            let image = zipzi_core::image::decode_png(&bytes).map_err(|e| Cmo3Error::Asset {
                asset_id: id.clone(),
                message: e.message,
            })?;
            if (image.width, image.height) != (width, height) {
                return Err(Cmo3Error::Asset {
                    asset_id: id.clone(),
                    message: "PNG dimensions differ from the asset descriptor".into(),
                });
            }
            let size = bytes.len();
            let path = self.embed(bytes);
            let resource = self.graph.add(
                N::new("CImageResource")
                    .attr("width", width)
                    .attr("height", height)
                    .attr("type", "INT_ARGB")
                    .attr("imageFileBuf_size", size)
                    .attr("previewFileBuf_size", 0)
                    .field("imageFileBuf", N::new("file").attr("path", path)),
            );
            let atlas_guid = self.new_guid("CTextureAtlasGuid");
            let atlas_name = format!("TextureAtlas{}", atlases.len() + 1);
            let cache = self.cache(resource.clone(), width, height);
            let atlas = self.graph.add(
                N::new("CTextureAtlas")
                    .field("name", s(&atlas_name))
                    .field("width", i(width))
                    .field("height", i(height))
                    .field("cachedAtlasImage", resource.clone())
                    .field("lockCachedAtlasImage", b(false))
                    .field("guid", atlas_guid.clone())
                    .field("modelImages", N::empty_list())
                    .field("cachedImageManager", cache),
            );
            atlases.push(atlas);
            let texture = self.graph.reserve("GTexture2D");
            let filter = N::new("FilterMode")
                .field(
                    "minFilter",
                    N::enumeration("MinFilter", "LINEAR_MIPMAP_LINEAR"),
                )
                .field("magFilter", N::enumeration("MagFilter", "LINEAR"))
                .field("owner", texture.clone());
            let texture_guid = self.new_guid("GTextureGuid");
            self.graph.set(
                &texture,
                N::new("GTexture2D")
                    .field(
                        "super",
                        N::new("GTexture")
                            .field("name", s(atlas_name))
                            .field("wrapMode", N::enumeration("WrapMode", "CLAMP_TO_BORDER"))
                            .field("filterMode", filter)
                            .field("guid", texture_guid)
                            .field("anisotropy", N::enumeration("Anisotropy", "ON")),
                    )
                    .field("srcImageResource", resource)
                    .field(
                        "transformImageResource01toLogical01",
                        texture_transform(width, height),
                    )
                    .field("mipmapLevel", i(64))
                    .field("isPremultiplied", b(true)),
            );
            textures.insert(
                id.clone(),
                TextureBinding {
                    texture,
                    atlas_guid,
                    uv_scale: texture_scale(width, height),
                },
            );
        }
        let manager = N::new("CTextureManager")
            .field(
                "textureList",
                N::new("TextureImageGroup").field("children", N::empty_list()),
            )
            .field("_rawImages", N::empty_list())
            .field("_modelImageGroups", N::empty_list())
            .field("_textureAtlases", N::list("carray_list", atlases))
            .field("isTextureInputModelImageMode", b(false))
            .field("previewReductionRatio", i(1))
            .field("artPathBrushUsingLayeredImageIds", N::empty_list());
        Ok(TextureSet {
            manager,
            bindings: textures,
        })
    }
}
