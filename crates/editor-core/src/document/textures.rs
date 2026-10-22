//! Loaded texture snapshots and asset-based reuse across history changes.
use super::*;

pub(super) struct DocumentTextures {
    textures: Arc<[SceneTexture]>,
    slots: HashMap<String, usize>,
    assets: HashMap<String, kasane_core::ImageAsset>,
}

impl DocumentTextures {
    pub(super) fn new(session: &AuthoringSession) -> Result<Self> {
        let textures = Self::load(session)?;
        Ok(Self {
            slots: textures
                .iter()
                .enumerate()
                .map(|(i, t)| (t.asset_id.clone(), i))
                .collect(),
            assets: session
                .asset_ids()
                .iter()
                .filter_map(|id| session.asset(id).map(|a| (id.clone(), a)))
                .collect(),
            textures,
        })
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

    fn load(session: &AuthoringSession) -> Result<Arc<[SceneTexture]>> {
        session
            .asset_ids()
            .iter()
            .filter(|id| session.asset(id).is_some_and(|a| a.content.is_some()))
            .map(|id| {
                let data = session.read_texture(id)?;
                Ok(SceneTexture {
                    asset_id: id.clone(),
                    cache_key: data.cache_key,
                    width: data.width,
                    height: data.height,
                    rgba: data.rgba,
                })
            })
            .collect::<Result<Vec<_>>>()
            .map(Into::into)
    }

    pub(super) fn refresh(&mut self, session: &AuthoringSession) -> Result<()> {
        let assets: HashMap<_, _> = session
            .asset_ids()
            .iter()
            .filter_map(|id| session.asset(id).map(|a| (id.clone(), a)))
            .collect();
        if assets == self.assets {
            return Ok(());
        }
        let textures = session
            .asset_ids()
            .iter()
            .filter(|id| assets.get(*id).is_some_and(|a| a.content.is_some()))
            .map(|id| {
                if assets.get(id).map(|a| &a.content) == self.assets.get(id).map(|a| &a.content)
                    && let Some(&slot) = self.slots.get(id)
                {
                    return Ok(self.textures[slot].clone());
                }
                let data = session.read_texture(id)?;
                Ok(SceneTexture {
                    asset_id: id.clone(),
                    cache_key: data.cache_key,
                    width: data.width,
                    height: data.height,
                    rgba: data.rgba,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.slots = textures
            .iter()
            .enumerate()
            .map(|(i, t)| (t.asset_id.clone(), i))
            .collect();
        self.textures = textures.into();
        self.assets = assets;
        Ok(())
    }
}
