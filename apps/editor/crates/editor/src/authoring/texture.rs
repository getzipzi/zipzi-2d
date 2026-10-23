//! PNG preparation runs off-thread; native tokens own resource incarnation and
//! content epochs, while the host guards document/workspace lifetimes.
use super::*;
use crate::properties::Choice;
use crate::ui::{
    compact,
    form_dialog::{self, FormDialogOptions, FormDialogState, FormDialogView},
};
use gpui_kit::component::{
    ActiveTheme,
    select::{SearchableVec, Select, SelectEvent, SelectState},
};
use kasane_sdk::{ImageLoadToken, ResourceEdit, ResourceEditPlan};

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
                self.editor.editing_context() == Some(editor_core::EditingContext::Modeling),
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
                    let owner = cx.weak_entity();
                    let asset = asset.ok_or_else(|| anyhow::anyhow!("Texture no longer exists"))?;
                    window.defer(cx, move |window, cx| {
                        if let Err(e) =
                            TextureDelete::open(owner.clone(), context, asset, window, cx)
                        {
                            let _ =
                                owner.update(cx, |v, cx| v.set_edit_error(authoring_error(&e), cx));
                        }
                    });
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
                .spawn(async move { kasane_sdk::prepare_image_load(&path) })
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
        image: anyhow::Result<kasane_sdk::PreparedImageContent>,
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

struct TextureDelete {
    owner: WeakEntity<EditorView>,
    context: AuthoringContext,
    asset: String,
    dialog: FormDialogState,
    replacement: Entity<SelectState<SearchableVec<Choice>>>,
    users: std::sync::Arc<[ObjectRef]>,
    plan: Option<ResourceEditPlan>,
    reason: Option<String>,
    _subscriptions: Vec<Subscription>,
}
impl TextureDelete {
    fn current(&self, cx: &App) -> bool {
        !self.dialog.closed()
            && self.owner.upgrade().is_some_and(|o| {
                let v = o.read(cx);
                !v.modal && AuthoringContext::capture(&v.editor) == Some(self.context)
            })
    }
    fn review(&mut self, cx: &mut Context<Self>) {
        self.plan = None;
        self.reason = None;
        if !self.current(cx) {
            self.dialog.set_validation(false, true);
            return;
        }
        let replacement = self
            .replacement
            .read(cx)
            .selected_value()
            .cloned()
            .filter(|id| !id.is_empty());
        let result = self
            .owner
            .upgrade()
            .unwrap()
            .read(cx)
            .editor
            .plan_resource_authoring(
                self.context.editing.document,
                ResourceEdit::Delete {
                    id: self.asset.clone(),
                    replacement: replacement.map(|id| id.into()),
                },
            );
        match result {
            Ok(p) => self.plan = Some(p),
            Err(e) => self.reason = Some(authoring_error(&e)),
        }
        self.dialog.set_validation(self.plan.is_some(), true);
        cx.notify();
    }
    fn open(
        owner: WeakEntity<EditorView>,
        context: AuthoringContext,
        asset: String,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        let entity = owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("Editor closed"))?;
        anyhow::ensure!(
            AuthoringContext::capture(&entity.read(cx).editor) == Some(context),
            t!("authoring.animation_stale")
        );
        let doc = entity.read(cx).editor.document().unwrap();
        let info = doc.texture_info(&asset)?;
        let mut choices = vec![Choice {
            label: t!("texture.no_replacement").into(),
            value: String::new(),
        }];
        let index = doc.object_index();
        choices.extend(
            doc.resource_collection()?
                .into_iter()
                .filter(|r| r.id != asset)
                .map(|r| Choice {
                    label: index.name(&r).to_owned().into(),
                    value: r.id,
                }),
        );
        let view = cx.new(|cx| {
            let replacement = cx.new(|cx| {
                SelectState::new(
                    SearchableVec::new(choices),
                    Some(gpui_kit::component::IndexPath::new(0)),
                    window,
                    cx,
                )
                .searchable(true)
            });
            let select = cx.subscribe_in(&replacement, window, |v: &mut Self, _, e, _, cx| {
                if matches!(e, SelectEvent::Confirm(_)) {
                    v.review(cx);
                }
            });
            let observe = cx.observe(&entity, |v: &mut Self, _, cx| {
                if !v.current(cx) {
                    v.plan = None;
                    v.dialog.set_validation(false, true);
                    cx.notify();
                }
            });
            Self {
                owner,
                context,
                asset,
                dialog: FormDialogState::new(cx),
                replacement,
                users: info.users.into(),
                plan: None,
                reason: None,
                _subscriptions: vec![select, observe],
            }
        });
        view.update(cx, |v, cx| v.review(cx));
        let focus = view.read(cx).dialog.cancel_focus.clone();
        form_dialog::open(
            view,
            FormDialogOptions {
                title: t!("texture.delete").into_owned(),
                submit_label: t!("texture.delete").into(),
                footer_id: "texture-delete-footer",
                cancel_id: "texture-delete-cancel",
                submit_id: "texture-delete-submit",
                initial_focus: focus,
            },
            window,
            cx,
        );
        Ok(())
    }
}
impl FormDialogView for TextureDelete {
    fn dialog(&self) -> &FormDialogState {
        &self.dialog
    }
    fn dialog_mut(&mut self) -> &mut FormDialogState {
        &mut self.dialog
    }
    fn error_message(e: &anyhow::Error) -> String {
        authoring_error(e)
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()> {
        anyhow::ensure!(self.current(cx), t!("authoring.animation_stale"));
        let plan = self
            .plan
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!(t!("texture.needs_replacement")))?;
        self.owner.update(cx, |v, cx| {
            let result = v
                .editor
                .execute_resource_authoring(self.context.editing.document, plan)?;
            v.texture_loads.remove(&self.asset);
            let mut candidates = v.editor.document().unwrap().resource_collection()?;
            candidates.extend(self.users.iter().cloned());
            v.present_authoring_result(
                self.context.editing.document,
                &result,
                AuthoringPresentation::AfterDelete { candidates },
                window,
                cx,
            )?;
            v.clear_edit_error(cx);
            v.redraw(cx);
            Ok::<_, anyhow::Error>(())
        })??;
        Ok(())
    }
}
impl Render for TextureDelete {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.current(cx);
        let mut body = div()
            .id("texture-delete-dialog")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_2()
            .child(t!("texture.delete_hint"));
        let index = self
            .owner
            .upgrade()
            .and_then(|o| o.read(cx).editor.document().map(|d| d.object_index()));
        body = body.child(format!("{}: {}", t!("editor.used_by"), self.users.len()));
        // Bound the review viewport, retaining every actual dependency.
        let users = self.users.clone();
        let height = compact::rem((users.len() as f32 * compact::ROW).min(180.));
        let users = div()
            .id("texture-delete-users")
            .test_support()
            .h(height)
            .flex_shrink_0()
            .child(
                uniform_list(
                    "texture-delete-user-list",
                    users.len(),
                    move |range, _, _| {
                        range
                            .map(|i| {
                                let user = &users[i];
                                let label = format!(
                                    "{} · {}",
                                    index
                                        .as_ref()
                                        .map_or(user.id.as_str(), |index| index.name(user)),
                                    user.id
                                );
                                let hint = label.clone();
                                div()
                                    .id(("texture-delete-user", i))
                                    .test_support()
                                    .h(compact::rem(compact::ROW))
                                    .text_xs()
                                    .truncate()
                                    .tooltip(move |window, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(hint.clone())
                                            .build(window, cx)
                                    })
                                    .child(label)
                            })
                            .collect()
                    },
                )
                .h_full(),
            );
        body = body.child(users).child(t!("texture.replacement")).child(
            compact::select(Select::new(&self.replacement))
                .id("texture-replacement")
                .disabled(!current),
        );
        if !current {
            body = body.child(t!("authoring.animation_stale"));
        }
        if let Some(reason) = self.reason.as_ref().or(self.dialog.error.as_ref()) {
            body = body.child(div().text_color(cx.theme().danger).child(reason.clone()));
        }
        body
    }
}
