//! Dispatch shared editor commands and coordinate their UI effects.
use super::*;
use crate::project::ImportKind;
use editor_core::{Command, CommandEffect, CommandId};
use gpui_kit::component::WindowExt;

impl EditorView {
    pub(crate) fn viewport_command(
        &self,
        id: CommandId,
        source: editor_core::CommandSource,
    ) -> Command {
        if source == editor_core::CommandSource::NativeMenu {
            match (id.spec().scope, self.input_router.menu_target.get()) {
                (
                    editor_core::CommandScope::Viewport,
                    Some(crate::input_routing::LocalTarget::Viewport(binding)),
                ) => return self.editor.command_for_viewport(id, binding, source),
                (
                    editor_core::CommandScope::Timeline,
                    Some(crate::input_routing::LocalTarget::Timeline(binding)),
                ) => return self.editor.command_for_timeline(id, binding, source),
                (
                    editor_core::CommandScope::Viewport | editor_core::CommandScope::Timeline,
                    Some(_),
                ) => {
                    return Command {
                        id,
                        source,
                        target: editor_core::CommandTarget::Unavailable,
                    };
                }
                _ => {}
            }
        }
        if id.spec().scope == editor_core::CommandScope::Timeline {
            return self
                .dock
                .as_ref()
                .and_then(|dock| self.editor.timeline_binding(dock.timeline_id))
                .map(|binding| self.editor.command_for_timeline(id, binding, source))
                .unwrap_or(Command {
                    id,
                    source,
                    target: editor_core::CommandTarget::Unavailable,
                });
        }
        if id.spec().scope == editor_core::CommandScope::Viewport {
            if let Some(viewport) = self.focused_viewport {
                return self
                    .editor
                    .viewport_binding(viewport)
                    .map(|binding| self.editor.command_for_viewport(id, binding, source))
                    .unwrap_or(editor_core::Command {
                        id,
                        source,
                        target: editor_core::CommandTarget::Unavailable,
                    });
            }
        }
        self.editor.command(id).with_source(source)
    }

    pub(super) fn run_command(
        &mut self,
        id: CommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_viewports();
        let command = self.viewport_command(id, editor_core::CommandSource::Direct);
        self.run_invocation(command, window, cx);
    }

    pub(super) fn run_invocation(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = command.id;
        if self.modal || window.has_active_dialog(cx) {
            return;
        }
        if let editor_core::CommandTarget::Viewport(binding) = command.target {
            if !self.viewport_displayed(binding.viewport, window, cx) {
                return;
            }
        }
        if !self.editor.command_available(command) {
            return;
        }
        let previous = self.editor.active_id();
        match self.editor.dispatch(command) {
            Ok(CommandEffect::Done) => {
                if matches!(
                    id,
                    CommandId::PlayPause
                        | CommandId::RewindAnimation
                        | CommandId::ResetAnimation
                        | CommandId::ClearExpression
                        | CommandId::TimelinePreviousFrame
                        | CommandId::TimelineNextFrame
                        | CommandId::TimelinePreviousKey
                        | CommandId::TimelineNextKey
                        | CommandId::TimelineStart
                        | CommandId::TimelineEnd
                ) {
                    self.animation_anchor = None;
                }
                if matches!(
                    id,
                    CommandId::SelectTool
                        | CommandId::GlueTool
                        | CommandId::HandTool
                        | CommandId::CancelGesture
                ) {
                    if let editor_core::CommandTarget::Viewport(binding) = command.target {
                        if let Some(surface) = self.viewport_surface(binding.viewport, cx) {
                            let token = surface.update(cx, |surface, _| surface.cancel_pointer());
                            if let Some(token) = token {
                                self.editor.cancel_viewport_interaction(token);
                            }
                            if command.source != editor_core::CommandSource::Shortcut {
                                window.focus(&surface.read(cx).focus_handle(cx), cx);
                            }
                        }
                    }
                }
                if self.editor.active_id() != previous {
                    window.focus(&self.focus, cx);
                    self.refresh_document(window, cx);
                } else {
                    self.redraw(cx);
                }
            }
            Ok(CommandEffect::Host(command)) => match command.id {
                CommandId::ModelingWorkspace => {
                    self.switch_workspace(editor_core::WorkspaceKind::Modeling, window, cx)
                }
                CommandId::AnimationWorkspace => {
                    self.switch_workspace(editor_core::WorkspaceKind::Animation, window, cx)
                }
                CommandId::Open => self.open(window, cx),
                CommandId::ImportModel3 => self.import(ImportKind::Model3, window, cx),
                CommandId::ImportPsd => self.import(ImportKind::Psd, window, cx),
                CommandId::Save => self.save(false, window, cx),
                CommandId::SaveAs => self.save(true, window, cx),
                CommandId::Close => self.close_active(window, cx),
                CommandId::Quit => self.close(window, cx),
                CommandId::GenerateMesh => {
                    if let Err(error) = self.editor.execute(EditorCommand::FinishEdit) {
                        self.edit_error = Some(error.to_string());
                    } else {
                        let owner = cx.weak_entity();
                        window.defer(cx, move |window, cx| {
                            if owner.upgrade().is_none_or(|owner| !owner.read(cx).editor.command_available(command)) { return; }
                            if let Err(error) = crate::ui::mesh_generation::MeshGenerationForm::open(
                                owner.clone(), command.document().unwrap(), window, cx,
                            ) {
                                if let Some(owner) = owner.upgrade() {
                                    owner.update(cx, |owner, cx| {
                                        owner.edit_error = Some(error.to_string());
                                        cx.notify();
                                    });
                                }
                            }
                        });
                    }
                    cx.notify();
                }
                CommandId::CreateParameter
                | CommandId::EditParameter
                | CommandId::DeleteParameter
                | CommandId::CreateParameterGroup
                | CommandId::EditParameterGroup
                | CommandId::DeleteParameterGroup => {
                    self.handle_parameter_command(command, window, cx);
                }
                id if editor_core::PanelKind::for_command(id, self.editor.workspace())
                    .is_some() =>
                {
                    let kind =
                        editor_core::PanelKind::for_command(id, self.editor.workspace()).unwrap();
                    if let Some(dock) = &mut self.dock {
                        dock.toggle(kind, window, cx);
                    }
                    cx.notify();
                }
                CommandId::ResetLayout => self.reset_layout(window, cx),
                CommandId::ResetShortcuts => {
                    match keybindings::reset(cx) {
                        Ok(()) => self.menu_state = Default::default(),
                        Err(e) => self.edit_error = Some(e.to_string()),
                    }
                    cx.notify();
                }
                CommandId::ShowShortcuts => keybindings::show(window, cx),
                _ => {}
            },
            Err(e) => {
                self.edit_error = Some(e.to_string());
                cx.notify();
            }
        }
    }
}
