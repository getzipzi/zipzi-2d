//! Dispatch shared editor commands and coordinate their UI effects.
use super::*;
use crate::project::ImportKind;
use editor_core::{CommandEffect, CommandId};
use gpui_kit::component::WindowExt;

impl EditorView {
    pub(super) fn run_command(
        &mut self,
        id: CommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal || window.has_active_dialog(cx) {
            return;
        }
        let command = self.editor.command(id);
        if !self.editor.command_available(command) {
            return;
        }
        let previous = self.editor.active_id();
        match self.editor.dispatch(command) {
            Ok(CommandEffect::Done) => {
                if matches!(
                    id,
                    CommandId::RewindAnimation
                        | CommandId::ResetAnimation
                        | CommandId::ClearExpression
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
                        dock.canvas.update(cx, |canvas, _| canvas.cancel_pointer());
                        window.focus(&dock.canvas.read(cx).focus_handle(cx), cx);
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
                            if let Err(error) = crate::ui::mesh_generation::MeshGenerationForm::open(
                                owner.clone(), command.document.unwrap(), window, cx,
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
                CommandId::MeshTree
                | CommandId::Deformers
                | CommandId::Properties
                | CommandId::Parameters
                | CommandId::DrawOrder
                | CommandId::Relations
                | CommandId::Keyforms
                | CommandId::BlendShapes
                | CommandId::Glue
                | CommandId::Motions
                | CommandId::AnimationInfo
                | CommandId::AnimationParameters
                | CommandId::Expressions
                | CommandId::Physics
                | CommandId::Pose
                | CommandId::Resources => {
                    if let Some(dock) = &mut self.dock {
                        dock.toggle(
                            match id {
                                CommandId::Motions
                                    if self.editor.workspace()
                                        == editor_core::WorkspaceKind::Animation =>
                                {
                                    docking::PanelKind::AnimationMotions
                                }
                                CommandId::AnimationInfo => docking::PanelKind::AnimationInfo,
                                CommandId::AnimationParameters => {
                                    docking::PanelKind::AnimationParameters
                                }
                                CommandId::Expressions
                                    if self.editor.workspace()
                                        == editor_core::WorkspaceKind::Animation =>
                                {
                                    docking::PanelKind::AnimationExpressions
                                }
                                CommandId::MeshTree => docking::PanelKind::MeshTree,
                                CommandId::Deformers => docking::PanelKind::Deformers,
                                CommandId::Parameters => docking::PanelKind::Parameters,
                                CommandId::Properties => docking::PanelKind::Properties,
                                _ => docking::PanelKind::Document(match id {
                                    CommandId::DrawOrder => {
                                        crate::panels::DocumentPanelKind::DrawOrder
                                    }
                                    CommandId::Relations => {
                                        crate::panels::DocumentPanelKind::Relations
                                    }
                                    CommandId::Keyforms => {
                                        crate::panels::DocumentPanelKind::Keyforms
                                    }
                                    CommandId::BlendShapes => {
                                        crate::panels::DocumentPanelKind::BlendShapes
                                    }
                                    CommandId::Glue => crate::panels::DocumentPanelKind::Glue,
                                    CommandId::Motions => crate::panels::DocumentPanelKind::Motions,
                                    CommandId::Expressions => {
                                        crate::panels::DocumentPanelKind::Expressions
                                    }
                                    CommandId::Physics => crate::panels::DocumentPanelKind::Physics,
                                    CommandId::Pose => crate::panels::DocumentPanelKind::Pose,
                                    _ => crate::panels::DocumentPanelKind::Resources,
                                }),
                            },
                            window,
                            cx,
                        );
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
