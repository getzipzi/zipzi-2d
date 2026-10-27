use std::collections::{HashMap, HashSet};
use zipzi_render::{TextureCatalog, TextureInfo};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuTargetConfig {
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
}

#[derive(Clone, Copy, Debug)]
pub struct WgpuTexture<'a> {
    /// Premultiplied RGBA/BGRA view, such as one created from [`crate::upload_rgba8`].
    pub view: &'a wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Default)]
pub struct WgpuTextureCatalog<'a> {
    textures: HashMap<String, WgpuTexture<'a>>,
    revisions: HashMap<String, u64>,
    repeating: HashSet<String>,
}

impl<'a> WgpuTextureCatalog<'a> {
    pub fn new(textures: HashMap<String, WgpuTexture<'a>>) -> Self {
        Self {
            textures,
            revisions: HashMap::new(),
            repeating: HashSet::new(),
        }
    }

    pub fn get(&self, id: &str) -> Option<&WgpuTexture<'a>> {
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

impl TextureCatalog for WgpuTextureCatalog<'_> {
    fn texture_info(&self, id: &str) -> Option<TextureInfo> {
        self.get(id).map(|texture| TextureInfo {
            width: texture.width,
            height: texture.height,
        })
    }
}
