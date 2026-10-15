//! A shared, bounded cache for both virtualized trees. Never rasterize in a UI callback.
use crate::EditorView;
use editor_core::{DocumentId, MESH_THUMBNAIL_SIZE, MeshTextureRegion};
use gpui_kit::*;
use std::{collections::HashMap, sync::Arc};

const CACHE_CAPACITY: usize = 512;
const BATCH_SIZE: usize = 16;

struct Entry {
    revision: u64,
    source: Option<MeshTextureRegion>,
    image: Option<Arc<RenderImage>>,
    used: u64,
}

#[derive(Default)]
pub(crate) struct MeshThumbnails {
    document: Option<DocumentId>,
    entries: HashMap<String, Entry>,
    clock: u64,
    task: Option<Task<()>>,
}

impl MeshThumbnails {
    pub fn new(cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            cx.on_release(|this: &mut Self, cx| {
                for entry in this.entries.values_mut() {
                    if let Some(image) = entry.image.take() {
                        cx.drop_image(image, None);
                    }
                }
            })
            .detach();
            Self::default()
        })
    }

    pub fn sync_document(
        &mut self,
        document: Option<DocumentId>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.document == document {
            return;
        }
        self.task = None;
        for (_, entry) in self.entries.drain() {
            if let Some(image) = entry.image {
                cx.drop_image(image, Some(window));
            }
        }
        self.document = document;
    }

    /// Called only for a virtual list's visible range. There is no scroll backlog:
    /// after each bounded batch the trees request their *current* visible rows again.
    pub fn request(
        &mut self,
        owner: &Entity<EditorView>,
        ids: &[&str],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Option<Arc<RenderImage>>> {
        self.clock += 1;
        let used = self.clock;
        // Protect the whole requested range before making room for any new entries.
        for id in ids {
            if let Some(entry) = self.entries.get_mut(*id) {
                entry.used = used;
            }
        }
        let mut retired = Vec::new();
        let state = owner.read(cx);
        if state.editor.active_id() != self.document {
            return vec![None; ids.len()];
        }
        if let Some(document) = state.editor.document() {
            let revision = document.mesh_texture_revision();
            for id in ids {
                if self
                    .entries
                    .get(*id)
                    .is_some_and(|entry| entry.revision == revision)
                {
                    continue;
                }
                let source = document.mesh_texture_region(id);
                if let Some(entry) = self.entries.get_mut(*id) {
                    if entry.source != source {
                        retired.extend(entry.image.take());
                        entry.source = source;
                    }
                    entry.revision = revision;
                } else {
                    if self.entries.len() >= CACHE_CAPACITY {
                        let oldest = self
                            .entries
                            .iter()
                            .filter(|(_, entry)| entry.used != used)
                            .min_by_key(|(_, entry)| entry.used)
                            .map(|(id, _)| id.clone());
                        let Some(oldest) = oldest else {
                            continue;
                        };
                        retired.extend(self.entries.remove(&oldest).unwrap().image);
                    }
                    self.entries.insert(
                        (*id).to_owned(),
                        Entry {
                            revision,
                            source,
                            image: None,
                            used,
                        },
                    );
                }
            }
        }
        let images = ids
            .iter()
            .map(|id| self.entries.get(*id).and_then(|entry| entry.image.clone()))
            .collect();
        let jobs: Vec<_> = if self.task.is_none() {
            ids.iter()
                .filter_map(|id| {
                    let entry = self.entries.get(*id)?;
                    if entry.image.is_some() {
                        return None;
                    }
                    Some(((*id).to_owned(), entry.source.clone()?))
                })
                .take(BATCH_SIZE)
                .collect()
        } else {
            Vec::new()
        };
        for image in retired {
            cx.drop_image(image, Some(window));
        }
        if !jobs.is_empty() {
            let document = self.document;
            let background = cx.background_executor().spawn(async move {
                jobs.into_iter()
                    .map(|(id, source)| {
                        let mut pixels = source.rgba();
                        // GPUI RenderImage stores straight BGRA, not RGBA.
                        for pixel in pixels.chunks_exact_mut(4) {
                            pixel.swap(0, 2);
                        }
                        let buffer = image::RgbaImage::from_raw(
                            MESH_THUMBNAIL_SIZE,
                            MESH_THUMBNAIL_SIZE,
                            pixels,
                        )
                        .unwrap();
                        let image = Arc::new(RenderImage::new([image::Frame::new(buffer)]));
                        (id, source, image)
                    })
                    .collect::<Vec<_>>()
            });
            self.task = Some(cx.spawn(async move |this, cx| {
                let completed = background.await;
                _ = this.update(cx, |this, cx| {
                    if this.document != document {
                        return;
                    }
                    for (id, source, image) in completed {
                        // Edits or eviction may have superseded this work while it ran.
                        if let Some(entry) = this.entries.get_mut(&id) {
                            if entry.source.as_ref() == Some(&source) {
                                entry.image = Some(image);
                            }
                        }
                    }
                    this.task = None;
                    cx.notify();
                });
            }));
        }
        images
    }
}

#[cfg(any(test, feature = "test-support"))]
impl MeshThumbnails {
    #[cfg(test)]
    pub(crate) fn cached_image(&self, id: &str) -> Option<Arc<RenderImage>> {
        self.entries.get(id)?.image.clone()
    }
    pub(crate) fn cache_state(&self) -> (usize, bool) {
        (self.entries.len(), self.task.is_some())
    }
}
