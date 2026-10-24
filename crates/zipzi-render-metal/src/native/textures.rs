//! Host-owned source texture views and their revision/sampling metadata.
use metal::TextureRef;
use std::collections::HashMap;
use zipzi_render::TextureCatalog;

pub struct MetalTexture<'a> {
    /// Premultiplied RGBA, as produced by `MetalContext::upload_rgba8`.
    pub view: &'a TextureRef,
    pub width: u32,
    pub height: u32,
}

pub struct MetalTextureCatalog<'a> {
    textures: HashMap<String, MetalTexture<'a>>,
    revisions: HashMap<String, u64>,
    repeating: std::collections::HashSet<String>,
}

impl<'a> MetalTextureCatalog<'a> {
    pub fn new(textures: HashMap<String, MetalTexture<'a>>) -> Self {
        Self {
            textures,
            revisions: HashMap::new(),
            repeating: Default::default(),
        }
    }

    pub fn get(&self, id: &str) -> Option<&MetalTexture<'a>> {
        self.textures.get(id)
    }

    pub fn set_repeat(&mut self, id: impl Into<String>, repeat: bool) {
        let id = id.into();
        if repeat {
            self.repeating.insert(id);
        } else {
            self.repeating.remove(&id);
        }
    }

    pub fn repeats(&self, id: &str) -> bool {
        self.repeating.contains(id)
    }

    pub fn set_revision(&mut self, id: impl Into<String>, revision: u64) {
        self.revisions.insert(id.into(), revision);
    }

    pub fn revision(&self, id: &str) -> Option<u64> {
        self.revisions.get(id).copied()
    }
}

impl TextureCatalog for MetalTextureCatalog<'_> {
    fn texture_info(&self, id: &str) -> Option<zipzi_render::TextureInfo> {
        self.get(id).map(|texture| zipzi_render::TextureInfo {
            width: texture.width,
            height: texture.height,
        })
    }
}
