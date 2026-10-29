//! PNG preparation runs off-thread; native tokens own resource incarnation and
//! content epochs, while the host guards document/workspace lifetimes.
use super::*;
use zipzi_sdk::{ImageLoadToken, ResourceEdit};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Control {
    New,
    Import,
    Load,
    Delete,
    Cancel,
}
impl Control {
    pub fn key(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Import => "import",
            Self::Load => "load",
            Self::Delete => "delete",
            Self::Cancel => "cancel",
        }
    }
    pub fn label(self) -> SharedString {
        let key = format!("texture.{}", self.key());
        t!(&key).into()
    }
}

pub(crate) struct TextureLoad {
    context: AuthoringContext,
    serial: uuid::Uuid,
    token: Option<ImageLoadToken>,
}
impl Drop for TextureLoad {
    fn drop(&mut self) {
        if let Some(t) = &self.token {
            t.cancel();
        }
    }
}
impl EditorView {
    pub(crate) fn pending_texture_imports(&self) -> Vec<String> {
        self.texture_loads
            .iter()
            .filter(|(_, r)| {
                r.token.is_none() && self.editor.editing_context_token() == Some(r.context.editing)
            })
            .map(|(id, _)| id.clone())
            .collect()
    }
    pub(crate) fn texture_loading(&self, asset: &str) -> bool {
        self.texture_loads
            .get(asset)
            .is_some_and(|r| self.editor.editing_context_token() == Some(r.context.editing))
    }
    pub(crate) fn run_texture_control(
        &mut self,
        context: AuthoringContext,
        asset: Option<String>,
        control: Control,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = (|| {
            anyhow::ensure!(
                !self.modal
                    && !window.has_active_dialog(cx)
                    && AuthoringContext::capture(&self.editor) == Some(context),
                t!("authoring.animation_stale")
            );
            anyhow::ensure!(
                self.editor.editing_context() == Some(zipzi_editor_core::EditingContext::Modeling),
                "Modeling context changed"
            );
            if control == Control::Cancel {
                if let Some(id) = asset {
                    self.texture_loads.remove(&id);
                }
                self.redraw(cx);
                return Ok(());
            }
            anyhow::ensure!(
                self.prepare_property_inputs(context.editing.document, window, cx),
                t!("authoring.candidate_active")
            );
            let context = AuthoringContext::capture(&self.editor).unwrap();
            match control {
                Control::New => {
                    let p = self.editor.plan_resource_authoring(
                        context.editing.document,
                        ResourceEdit::Create {
                            id: uuid::Uuid::new_v4().to_string(),
                            name: t!("texture.default_name").into_owned(),
                            before: None,
                        },
                    )?;
                    let result = self
                        .editor
                        .execute_resource_authoring(context.editing.document, &p)?;
                    self.present_authoring_result(
                        context.editing.document,
                        &result,
                        AuthoringPresentation::Created {
                            kind: ObjectKind::Texture,
                            projection: None,
                        },
                        window,
                        cx,
                    )?;
                    self.clear_edit_error(cx);
                    self.redraw(cx);
                }
                Control::Import | Control::Load => self.pick_texture(context, asset, window, cx)?,
                Control::Delete => {
                    let asset = asset.ok_or_else(|| anyhow::anyhow!("Texture no longer exists"))?;
                    self.run_object_deletion(
                        context,
                        &[ObjectRef::new(ObjectKind::Texture, asset)],
                        window,
                        cx,
                    );
                }
                Control::Cancel => unreachable!(),
            }
            Ok::<_, anyhow::Error>(())
        })();
        if let Err(e) = result {
            self.set_edit_error(authoring_error(&e), cx);
        }
    }
    fn pick_texture(
        &mut self,
        context: AuthoringContext,
        asset: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let token = asset
            .as_ref()
            .map(|id| self.editor.image_load_token(context.editing.document, id))
            .transpose()?;
        let key = asset
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let serial = uuid::Uuid::new_v4();
        self.texture_loads.insert(
            key.clone(),
            TextureLoad {
                context,
                serial,
                token,
            },
        );
        window.focus(&self.focus, cx);
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(t!("texture.choose_png").into()),
        });
        self.modal = true;
        self.redraw(cx);
        cx.spawn_in(window, async move |this, cx| {
            let picked = picker.await;
            let path = this
                .update_in(cx, |v, _, cx| {
                    v.modal = false;
                    let path = match picked {
                        Ok(Ok(Some(paths))) => paths.into_iter().next(),
                        Ok(Ok(None)) => None,
                        other => {
                            v.set_edit_error(format!("{other:?}"), cx);
                            None
                        }
                    };
                    if path.is_none() {
                        v.texture_loads.remove(&key);
                    }
                    v.redraw(cx);
                    path
                })
                .ok()
                .flatten();
            let Some(path) = path else { return };
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let image = cx
                .background_executor()
                .spawn(async move { zipzi_sdk::prepare_image_load(&path) })
                .await;
            let _ = this.update_in(cx, |v, window, cx| {
                v.finish_texture_load(
                    &key,
                    serial,
                    asset.is_none().then_some(name),
                    image.map_err(Into::into),
                    window,
                    cx,
                );
            });
        })
        .detach();
        Ok(())
    }
    fn finish_texture_load(
        &mut self,
        key: &str,
        serial: uuid::Uuid,
        imported_name: Option<String>,
        image: anyhow::Result<zipzi_sdk::PreparedImageContent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .texture_loads
            .get(key)
            .is_none_or(|r| r.serial != serial)
        {
            return;
        }
        let request = self.texture_loads.remove(key).unwrap();
        // Import has no existing resource token: conservatively reject intervening
        // edits too. Existing resources use the narrower native content epoch.
        if self.editor.editing_context_token() != Some(request.context.editing)
            || (request.token.is_none()
                && AuthoringContext::capture(&self.editor) != Some(request.context))
        {
            self.redraw(cx);
            return;
        }
        let result = (|| {
            let image = image?;
            let document = request.context.editing.document;
            let result = if let Some(token) = &request.token {
                self.editor
                    .replace_prepared_image_content(document, token, &image)?
            } else {
                self.editor.import_prepared_texture(
                    document,
                    key.to_owned(),
                    imported_name.unwrap_or_default(),
                    &image,
                )?
            };
            if request.token.is_none() {
                self.present_authoring_result(
                    document,
                    &result,
                    AuthoringPresentation::Created {
                        kind: ObjectKind::Texture,
                        projection: None,
                    },
                    window,
                    cx,
                )?;
            }
            self.clear_edit_error(cx);
            Ok::<_, anyhow::Error>(())
        })();
        if let Err(e) = result {
            self.set_edit_error(authoring_error(&e), cx);
        }
        self.redraw(cx);
    }
}
