//! Deletion preview, choices and optional technical report.
use super::labels::*;
use super::*;
use gpui_kit::component::{ActiveTheme, button::ButtonVariants};

fn technical(section: S) -> bool {
    matches!(
        section,
        S::Rewritten
            | S::Owned
            | S::Deformation
            | S::Organization
            | S::GuaranteeLimits
            | S::ConversionDomain
            | S::Relations
    )
}

fn section_hint(section: S) -> String {
    t!(match section {
        S::Removed => "deletion.removed_hint",
        S::Rewritten => "deletion.rewritten_hint",
        S::Effects => "deletion.effects_hint",
        S::Decisions => "deletion.decisions_hint",
        S::Locks => "deletion.locks_hint",
        S::Structure => "deletion.structure_hint",
        S::Exports => "deletion.exports_hint",
        _ => "deletion.technical_hint",
    })
    .into_owned()
}

impl DeletionDialog {
    fn decision_inputs(&self, choices: &[C]) -> AnyElement {
        let mut inputs = div().flex().flex_col().gap_2();
        if choices
            .iter()
            .any(|c| matches!(c, C::ChoosePart | C::SupplyOffscreenMapping))
        {
            inputs = inputs
                .child(compact::select(Select::new(&self.destination)))
                .child(t!("deletion.mapping_hint"))
                .child(compact::input(Input::new(&self.mapping)));
        }
        if choices.contains(&C::ChooseTexture) {
            inputs = inputs
                .child(t!("deletion.choose_texture"))
                .child(compact::select(Select::new(&self.texture)).id("deletion-texture"));
        }
        if choices.contains(&C::ChoosePhysicsNode) {
            inputs = inputs
                .child(t!("deletion.choose_node"))
                .child(compact::select(Select::new(&self.node)).id("deletion-node"));
        }
        if choices.contains(&C::ChooseBlendBase) {
            inputs = inputs
                .child(t!("deletion.choose_base"))
                .child(compact::select(Select::new(&self.base)).id("deletion-base"));
        }
        inputs.into_any_element()
    }

    fn summary(&self, cx: &App) -> AnyElement {
        let mut summary = div()
            .id("deletion-summary")
            .test_support()
            .flex()
            .flex_col()
            .gap_1();
        let title = if self.working {
            t!("deletion.checking").into_owned()
        } else if !self.current(cx) {
            t!("deletion.stale").into_owned()
        } else if self.plan.is_some() {
            let single = self
                .plan
                .as_ref()
                .and_then(|plan| plan.page(S::Removed, 0, 1).ok())
                .filter(|_| self.count(S::Removed) == 1)
                .and_then(|page| page.rows.into_iter().next())
                .and_then(|row| match row {
                    R::Removal { removal } => Some(self.name(&removal.object, cx)),
                    _ => None,
                });
            if let Some(name) = single {
                t!("deletion.summary_named", name = name).into_owned()
            } else {
                t!(
                    if self.count(S::Removed) == 1 {
                        "deletion.summary_single"
                    } else {
                        "deletion.summary_count"
                    },
                    count = self.count(S::Removed)
                )
                .into_owned()
            }
        } else {
            t!("deletion.check_failed").into_owned()
        };
        summary = summary.child(
            div()
                .id(if self.working {
                    "deletion-planning"
                } else {
                    "deletion-summary-title"
                })
                .test_support()
                .aria_label(title.clone())
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        );
        if self.plan.is_some() {
            let hint = if self.executable() {
                "deletion.ready_hint"
            } else {
                "deletion.blocked_hint"
            };
            summary = summary.child(
                div()
                    .id("deletion-status")
                    .test_support()
                    .text_color(if self.executable() {
                        cx.theme().muted_foreground
                    } else {
                        cx.theme().warning
                    })
                    .child(t!(hint).into_owned()),
            );
            if self.details && self.count(S::Owned) > 0 {
                summary =
                    summary.child(div().text_color(cx.theme().muted_foreground).child(
                        t!("deletion.owned_count", count = self.count(S::Owned)).into_owned(),
                    ));
            }
        }
        summary.into_any_element()
    }

    fn render_scope(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use zipzi_sdk::DeletionTarget as T;
        let roots: Vec<_> = self
            .intent
            .request
            .roots
            .iter()
            .enumerate()
            .filter_map(|(i, root)| {
                let (other, keep) = match root {
                    T::Part(id) => (T::DissolvePart(id.clone()), false),
                    T::DissolvePart(id) => (T::Part(id.clone()), true),
                    T::Deformer(id) => (T::RemoveDeformer(id.clone()), false),
                    T::RemoveDeformer(id) => (T::Deformer(id.clone()), true),
                    T::ParameterFolder(id) => (T::ParameterFolderContents(id.clone()), true),
                    T::ParameterFolderContents(id) => (T::ParameterFolder(id.clone()), false),
                    _ => return None,
                };
                Some((i, root.clone(), other, keep))
            })
            .collect();
        if roots.is_empty() {
            return None;
        }
        let current = self.current(cx);
        let mut scope = div()
            .id("deletion-scope-options")
            .test_support()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child(t!("deletion.scope_options")),
            );
        for (i, root, other, keep) in roots.iter().skip(self.scope_offset).take(4).cloned() {
            let mut choices = div().flex().flex_wrap().gap_1();
            for desired_keep in [false, true] {
                let target = if desired_keep == keep {
                    root.clone()
                } else {
                    other.clone()
                };
                let id = if desired_keep == keep {
                    "deletion-scope-current"
                } else {
                    "deletion-scope"
                };
                choices = choices.child(
                    compact::toggle(
                        Button::new((id, i)).label(
                            t!(if desired_keep {
                                "deletion.scope_keep"
                            } else {
                                "deletion.scope_delete"
                            })
                            .into_owned(),
                        ),
                        desired_keep == keep,
                    )
                    .disabled(!current || self.working)
                    .on_click(cx.listener(move |v, _, window, cx| {
                        if v.intent.request.roots[i] != target {
                            v.intent.request.roots[i] = target.clone();
                            v.review(window, cx);
                        }
                    })),
                );
            }
            scope = scope.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(self.name(&root.object(), cx))
                    .child(choices),
            );
        }
        if roots.len() > 4 {
            let previous = self.scope_offset.saturating_sub(4);
            let next = self.scope_offset + 4;
            scope = scope.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        compact::button(Button::new("deletion-scope-previous"))
                            .label(t!("deletion.previous"))
                            .disabled(self.scope_offset == 0)
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.scope_offset = previous;
                                cx.notify();
                            })),
                    )
                    .child(
                        compact::button(Button::new("deletion-scope-next"))
                            .label(t!("deletion.next"))
                            .disabled(next >= roots.len())
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.scope_offset = next;
                                cx.notify();
                            })),
                    ),
            );
        }
        Some(scope.into_any_element())
    }

    fn export_message(&self, failure: &zipzi_sdk::DeletionExportFailure, cx: &App) -> String {
        use zipzi_sdk::DeletionExportFailure::*;
        let name = match failure {
            Model3NamespaceChanged => "model3".to_owned(),
            Cdi { .. } => "CDI".to_owned(),
            Motion { id, .. } | Expression { id, .. } | Pose { id, .. } | Physics { id, .. } => {
                self.name_id(id, cx)
            }
        };
        format!("{name} · {}", t!("deletion.export_unavailable"))
    }
}

impl Render for DeletionDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.current(cx);
        let mut body = div()
            .id("tree-deletion-dialog")
            .test_support()
            .track_focus(&self.dialog.focus)
            .flex()
            .flex_col()
            .gap_3()
            .child(self.summary(cx));
        if let Some(scope) = self.render_scope(cx) {
            body = body.child(scope);
        }
        let parameters: Vec<_> = self
            .intent
            .request
            .roots
            .iter()
            .filter_map(|root| match root {
                zipzi_sdk::DeletionTarget::Parameter(id) => Some(id.clone()),
                _ => None,
            })
            .collect();
        if !parameters.is_empty() {
            body = body.child(
                compact::toggle(
                    Button::new("deletion-options").label(t!("deletion.parameter_options")),
                    self.options,
                )
                .on_click(cx.listener(|v, _, _, cx| {
                    v.options = !v.options;
                    cx.notify();
                })),
            );
        }
        if self.options && !parameters.is_empty() {
            body = body
                .child(t!("deletion.slice"))
                .child(compact::input(Input::new(&self.slice)).id("deletion-slice"));
            for (i, id) in parameters.into_iter().enumerate() {
                body = body.child(
                    compact::button(Button::new(("deletion-slice-apply", i)))
                        .label(format!(
                            "{} · {}",
                            self.name_id(&id, cx),
                            t!("deletion.apply_slice")
                        ))
                        .disabled(!current || self.working)
                        .on_click(cx.listener(move |v, _, window, cx| {
                            let value = v.slice.read(cx).value();
                            if value.trim().is_empty() {
                                v.intent.request.parameter_slices.remove(&id);
                            } else {
                                match value.trim().parse::<f32>() {
                                    Ok(value) if value.is_finite() => {
                                        v.intent.request.parameter_slices.insert(id.clone(), value);
                                    }
                                    _ => {
                                        v.dialog.error =
                                            Some(t!("binding.invalid_number").into_owned());
                                        cx.notify();
                                        return;
                                    }
                                }
                            }
                            v.review(window, cx);
                        })),
                );
            }
        }
        if let Some(plan) = &self.plan {
            let sections = plan.summary().sections.clone();
            let mut tabs = div().flex().flex_wrap().gap_1();
            for (i, section) in sections
                .iter()
                .enumerate()
                .filter(|(_, s)| s.count > 0 && !technical(s.section))
            {
                let value = section.section;
                tabs = tabs.child(
                    compact::toggle(
                        Button::new(("deletion-section", i)).label(format!(
                            "{} ({})",
                            section_label(value),
                            section.count
                        )),
                        self.section == value,
                    )
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.select_section(value);
                        cx.notify();
                    })),
                );
            }
            if self.details
                || sections
                    .iter()
                    .filter(|s| s.count > 0 && !technical(s.section))
                    .count()
                    > 1
            {
                body = body.child(tabs);
            }
            if !matches!(self.section, S::Removed | S::Decisions) {
                body = body.child(
                    div()
                        .id("deletion-section-hint")
                        .test_support()
                        .text_color(cx.theme().muted_foreground)
                        .child(section_hint(self.section)),
                );
            }
            if self.details && sections.iter().any(|s| s.count > 0 && technical(s.section)) {
                let mut tabs = div()
                    .id("deletion-technical-tabs")
                    .test_support()
                    .flex()
                    .flex_wrap()
                    .gap_1();
                for (i, section) in sections
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.count > 0 && technical(s.section))
                {
                    let value = section.section;
                    tabs = tabs.child(
                        compact::toggle(
                            Button::new(("deletion-section", i)).label(format!(
                                "{} ({})",
                                section_label(value),
                                section.count
                            )),
                            self.section == value,
                        )
                        .on_click(cx.listener(move |v, _, _, cx| {
                            v.select_section(value);
                            cx.notify();
                        })),
                    );
                }
                body = body.child(tabs);
            }
        }
        if let Some(page) = &self.page {
            // A page shares one replacement selection; render each state only once.
            if self.section == S::Decisions && page.rows.len() > 1 {
                let choices: Vec<_> = page
                    .rows
                    .iter()
                    .filter_map(|row| match row {
                        R::Decision { decision } => Some(decision.choices.iter().cloned()),
                        _ => None,
                    })
                    .flatten()
                    .collect();
                body = body.child(self.decision_inputs(&choices));
            }
            let mut rows = div()
                .id("deletion-rows")
                .test_support()
                .max_h(compact::scaled(240., cx))
                .flex_shrink_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_2();
            for (i, row) in page.rows.clone().into_iter().enumerate() {
                let text = match &row {
                    R::Removal { removal } => self.name(&removal.object, cx),
                    R::Relation { relation } => format!(
                        "{} · {} → {}",
                        self.name(&relation.owner, cx),
                        field_label(&relation.field),
                        self.name(&relation.target, cx)
                    ),
                    R::Effect { effect } => effect_label(effect, |o| self.name(o, cx)),
                    R::Decision { decision } => format!(
                        "{} · {}",
                        self.name(&decision.object, cx),
                        decision_label(&decision.reason, self.details)
                    ),
                    R::Lock { blocker } => format!(
                        "{} · {}: {}",
                        self.name(&blocker.object, cx),
                        t!("deletion.lock_source"),
                        self.name_id(&blocker.source_id, cx)
                    ),
                    R::Structural { blocker } => {
                        let reason = if self.details {
                            format!("{}: {}", blocker.code, blocker.reason)
                        } else if blocker.code == "LAST_MOTION_REGISTRATION" {
                            t!("deletion.last_registration").into_owned()
                        } else {
                            t!("deletion.invalid_structure").into_owned()
                        };
                        format!("{} · {reason}", self.name_id(&blocker.object_id, cx))
                    }
                    R::Deformation {
                        object,
                        parent_id,
                        strategy,
                        guarantee,
                    } => format!(
                        "{} → {} · {} · {}",
                        self.name(object, cx),
                        parent_id,
                        strategy_label(strategy),
                        guarantee
                            .as_ref()
                            .map(|g| format!(
                                "{}: {} · {}: {}, {} · {}",
                                t!("deletion.error"),
                                g.error_bound,
                                t!("deletion.tolerances"),
                                g.tolerance,
                                g.relative_tolerance,
                                t!("deletion.position_only")
                            ))
                            .unwrap_or_else(|| t!("deletion.unpreserved").into_owned())
                    ),
                    R::Organization {
                        object,
                        parent_id,
                        mapping_slots,
                    } => format!(
                        "{} → {} · {}: {}",
                        self.name(object, cx),
                        parent_id,
                        t!("deletion.mapping_slots"),
                        mapping_slots.unwrap_or(0)
                    ),
                    R::GuaranteeLimit { object, ancestor } => format!(
                        "{} · {}: {}",
                        self.name(object, cx),
                        t!("deletion.limited_by"),
                        self.name(ancestor, cx)
                    ),
                    R::Domain {
                        object,
                        parameter_id,
                        minimum,
                        maximum,
                        reference_value,
                    } => format!(
                        "{} · {} [{minimum}, {maximum}] · {reference_value}",
                        self.name(object, cx),
                        parameter_id
                    ),
                    R::Export { failure, existing } => format!(
                        "{} · {}",
                        if *existing {
                            t!("deletion.existing_export")
                        } else {
                            t!("deletion.new_export")
                        },
                        if self.details {
                            export_label(failure)
                        } else {
                            self.export_message(failure, cx)
                        }
                    ),
                };
                let mut item = div()
                    .id(("deletion-row", i))
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .py_2()
                    .aria_label(text.clone())
                    .child(text);
                if let R::Removal { removal } = &row {
                    let reason = t!(match removal.reason {
                        zipzi_sdk::DeletionReason::Requested => "deletion.reason_requested",
                        zipzi_sdk::DeletionReason::Scope => "deletion.reason_child",
                        zipzi_sdk::DeletionReason::Owned => "deletion.reason_owned",
                        zipzi_sdk::DeletionReason::Dependency => "deletion.reason_dependency",
                    })
                    .into_owned();
                    item = item.child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .text_size(compact::rem(compact::CAPTION))
                            .child(format!(
                                "{} · {reason}",
                                self.object_type(&removal.object, cx)
                            )),
                    );
                }
                if self.details {
                    let ids = match &row {
                        R::Removal { removal } => Some(removal.object.id.clone()),
                        R::Relation { relation } => {
                            Some(format!("{} → {}", relation.owner.id, relation.target.id))
                        }
                        R::Decision { decision } => Some(decision.object.id.clone()),
                        R::Lock { blocker } => {
                            Some(format!("{} · {}", blocker.object.id, blocker.source_id))
                        }
                        _ => None,
                    };
                    if let Some(ids) = ids {
                        item = item.child(
                            div()
                                .id(("deletion-row-id", i))
                                .test_support()
                                .aria_label(ids.clone())
                                .text_color(cx.theme().muted_foreground)
                                .text_size(compact::rem(compact::CAPTION))
                                .child(ids),
                        );
                    }
                }
                if let R::Decision { decision } = row {
                    if page.rows.len() == 1 {
                        item = item.child(self.decision_inputs(&decision.choices));
                    }
                    let mut buttons = div().flex().flex_wrap().gap_1();
                    for (j, choice) in decision.choices.into_iter().enumerate() {
                        if choice == C::Cancel {
                            continue;
                        }
                        let object = decision.object.clone();
                        buttons = buttons.child(
                            compact::button(Button::new(SharedString::from(format!(
                                "deletion-choice-{i}-{j}"
                            ))))
                            .label(if choice == C::DeleteSubtree && object.kind == K::Motion {
                                t!("deletion.include_motion").into_owned()
                            } else {
                                choice_label(&choice)
                            })
                            .disabled(!current || self.working || !self.choice_ready(&choice, cx))
                            .on_click(cx.listener(
                                move |v, _, window, cx| {
                                    v.resolve(object.clone(), choice.clone(), window, cx)
                                },
                            )),
                        );
                    }
                    item = item.child(buttons);
                } else if let R::Export {
                    failure,
                    existing: false,
                } = row
                {
                    if !self.intent.accepted_export_failures.contains(&failure) {
                        item = item.child(
                            compact::button(Button::new(("deletion-accept-export", i)))
                                .label(t!("deletion.accept_export"))
                                .disabled(!current || self.working)
                                .on_click(cx.listener(move |v, _, window, cx| {
                                    v.intent.accepted_export_failures.push(failure.clone());
                                    v.review(window, cx);
                                })),
                        );
                    }
                } else if let R::Lock { blocker } = row {
                    let unlock = blocker.source_id.clone();
                    item = item.child(
                        compact::button(Button::new(("deletion-unlock", i)))
                            .label(t!("deletion.unlock"))
                            .disabled(!current || self.working)
                            .on_click(cx.listener(move |v, _, window, cx| {
                                let Some(plan) = v.plan.clone() else { return };
                                v.invalidate();
                                let result = v.owner.update(
                                    cx,
                                    |view, cx| -> anyhow::Result<AuthoringContext> {
                                        view.editor.unlock_deletion_source(
                                            v.context.editing.document,
                                            &plan,
                                            &unlock,
                                        )?;
                                        view.redraw(cx);
                                        AuthoringContext::capture(&view.editor).ok_or_else(|| {
                                            anyhow::anyhow!(t!("authoring.move_stale"))
                                        })
                                    },
                                );
                                match result {
                                    Ok(Ok(context)) => {
                                        v.context = context;
                                        v.preview_revision = None;
                                        v.review(window, cx);
                                    }
                                    Ok(Err(error)) => {
                                        v.dialog.error = Some(authoring_error(&error))
                                    }
                                    Err(error) => v.dialog.error = Some(error.to_string()),
                                }
                            })),
                    );
                    let source = blocker.source_id.clone();
                    item = item.child(
                        compact::button(Button::new(("deletion-locate-lock", i)))
                            .label(t!("deletion.locate"))
                            .disabled(!current || self.working)
                            .on_click(cx.listener(move |v, _, _, cx| {
                                let _ = v.owner.update(cx, |view, cx| {
                                    if let Some(object) = view
                                        .editor
                                        .document()
                                        .unwrap()
                                        .object_index()
                                        .objects
                                        .iter()
                                        .find(|o| o.reference.id == source)
                                        .map(|o| o.reference.clone())
                                    {
                                        let _ = view.editor.inspect_authoring_object(
                                            v.context.editing.document,
                                            object,
                                        );
                                        view.redraw(cx);
                                    }
                                });
                            })),
                    );
                }
                if self.details || self.section != S::Removed || self.count(S::Removed) != 1 {
                    rows = rows.child(item);
                }
            }
            let offset = page.offset;
            let previous = offset.saturating_sub(16);
            let next = page.next_offset;
            if self.details || self.section != S::Removed || self.count(S::Removed) != 1 {
                body = body.child(rows);
            }
            if page.total > 16 {
                let label = t!(
                    "deletion.page",
                    start = offset + 1,
                    end = offset + page.rows.len(),
                    total = page.total
                )
                .into_owned();
                body = body.child(
                    div()
                        .id("deletion-page")
                        .test_support()
                        .aria_label(label.clone())
                        .flex()
                        .gap_2()
                        .child(label)
                        .child(
                            compact::button(Button::new("deletion-previous"))
                                .label(t!("deletion.previous"))
                                .disabled(offset == 0)
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    v.offset = previous;
                                    v.load_page();
                                    cx.notify();
                                })),
                        )
                        .child(
                            compact::button(Button::new("deletion-next"))
                                .label(t!("deletion.next"))
                                .disabled(next.is_none())
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    if let Some(next) = next {
                                        v.offset = next;
                                        v.load_page();
                                        cx.notify();
                                    }
                                })),
                        ),
                );
            }
        }
        let details = self.details;
        let mut tools = div().flex().flex_wrap().items_center().gap_2().child(
            compact::button(
                Button::new("deletion-details").label(
                    t!(if details {
                        "deletion.hide_details"
                    } else {
                        "deletion.show_details"
                    })
                    .into_owned(),
                ),
            )
            .ghost()
            .text_color(cx.theme().muted_foreground)
            .on_click(cx.listener(|v, _, _, cx| {
                v.details = !v.details;
                if !v.details && technical(v.section) {
                    v.select_section(S::Removed);
                }
                cx.notify();
            })),
        );
        if details || self.dialog.error.is_some() || !current {
            tools = tools.child(
                compact::button(Button::new("deletion-review"))
                    .label(t!("deletion.recheck"))
                    .disabled(!self.source_current(cx) || self.working)
                    .on_click(cx.listener(|v, _, window, cx| v.review(window, cx))),
            );
        }
        body = body.child(tools);
        if let Some(error) = &self.dialog.error {
            body = body.child(
                div()
                    .id("deletion-error")
                    .test_support()
                    .aria_label(error.clone())
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        body
    }
}
