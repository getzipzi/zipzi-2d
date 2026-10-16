//! Dispatch shared editor commands and coordinate their UI effects.
use super::*;
use crate::project::ImportKind;
use editor_core::{Command, CommandEffect, CommandId};
use gpui_kit::component::WindowExt;

impl EditorView {
    pub(super) fn run_command(
        &mut self,
        id: CommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_viewports();
        let command = self.editor.command(id);
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
        if matches!(command.target, editor_core::CommandTarget::Viewport(_))
            && self
                .dock
                .as_ref()
                .is_none_or(|dock| !dock.viewport_displayed(cx))
        {
            return;
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
                    if let Some(dock) = &self.dock {
                        self.editor.cancel_gesture();
                        let token = dock
                            .active_canvas()
                            .update(cx, |canvas, _| canvas.cancel_pointer());
                        if let Some(token) = token {
                            self.editor.cancel_viewport_interaction(token);
                        }
                        window.focus(&dock.active_canvas().read(cx).focus_handle(cx), cx);
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
