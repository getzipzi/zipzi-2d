//! Captured texture versions and asset-based reuse across immutable snapshots.
use super::*;

pub(super) struct DocumentTextures {
    textures: Arc<[SceneTexture]>,
    slots: HashMap<String, usize>,
    assets: HashMap<String, zipzi_core::state::AssetRecord>,
}

impl DocumentTextures {
    pub(super) fn new(snapshot: &zipzi_sdk::Snapshot) -> Result<Self> {
        let empty = Self {
            textures: Arc::from([]),
            slots: HashMap::new(),
            assets: HashMap::new(),
        };
        Ok(empty.prepare_refresh(snapshot)?.unwrap_or(empty))
    }

    pub(super) fn snapshot(&self) -> Arc<[SceneTexture]> {
        self.textures.clone()
    }
    pub(super) fn get(&self, asset: &str) -> Option<&SceneTexture> {
        self.slots.get(asset).map(|&slot| &self.textures[slot])
    }
    pub(super) fn contains(&self, asset: &str) -> bool {
        self.slots.contains_key(asset)
    }

    pub(super) fn refresh(&mut self, snapshot: &zipzi_sdk::Snapshot) -> Result<()> {
        if let Some(next) = self.prepare_refresh(snapshot)? {
            *self = next;
        }
        Ok(())
    }

    /// Stage the complete texture snapshot before committing candidate content.
    /// A failed read must not leave a committed model paired with old pixels.
    pub(in crate::document) fn prepare_refresh(
        &self,
        snapshot: &zipzi_sdk::Snapshot,
    ) -> Result<Option<Self>> {
        let model = snapshot.document();
        let assets: HashMap<_, _> = model
            .asset_order()
            .iter()
            .filter_map(|id| model.asset(id).map(|a| (id.clone(), a.clone())))
            .collect();
        let order_unchanged = model
            .asset_order()
            .iter()
            .filter(|id| {
                matches!(
                    assets[*id].content,
                    zipzi_core::state::AssetContent::Image { .. }
                )
            })
            .eq(self.textures.iter().map(|t| &t.asset_id));
        if assets == self.assets && order_unchanged {
            return Ok(None);
        }
        let textures = model
            .asset_order()
            .iter()
            .filter(|id| {
                assets.get(*id).is_some_and(|a| {
                    matches!(a.content, zipzi_core::state::AssetContent::Image { .. })
                })
            })
            .map(|id| {
                if assets.get(id).map(|a| &a.content) == self.assets.get(id).map(|a| &a.content)
                    && let Some(&slot) = self.slots.get(id)
                {
                    return Ok(self.textures[slot].clone());
                }
                let data = snapshot.resources().read_texture(&assets[id])?;
                Ok(SceneTexture {
                    asset_id: id.clone(),
                    cache_key: data.cache_key,
                    width: data.width,
                    height: data.height,
                    rgba: data.rgba,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let slots = textures
            .iter()
            .enumerate()
            .map(|(i, t)| (t.asset_id.clone(), i))
            .collect();
        let unchanged_pixels = textures.len() == self.textures.len()
            && textures.iter().zip(self.textures.iter()).all(|(a, b)| {
                a.asset_id == b.asset_id
                    && a.cache_key == b.cache_key
                    && a.width == b.width
                    && a.height == b.height
                    && Arc::ptr_eq(&a.rgba, &b.rgba)
            });
        Ok(Some(Self {
            slots,
            textures: if unchanged_pixels {
                self.textures.clone()
            } else {
                textures.into()
            },
            assets,
        }))
    }
}
