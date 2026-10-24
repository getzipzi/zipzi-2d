//! Native menu structure and state derived from the core command catalog.
use super::*;
use crate::actions::*;
use gpui_kit::component::input;
use std::path::{Path, PathBuf};
use zipzi_editor_core::{COMMANDS, CommandId};

#[derive(Clone, Default, PartialEq, Eq)]
pub struct MenuState {
    pub commands: Vec<zipzi_editor_core::CommandState>,
    pub creation_reasons: Vec<Option<String>>,
    pub targets: Vec<zipzi_editor_core::CommandTarget>,
    pub text: bool,
    pub modal: bool,
    pub recent_files: Vec<PathBuf>,
    pub language: Option<String>,
    pub deformer_visibility: zipzi_editor_core::DeformerVisibility,
    pub workspace: zipzi_editor_core::WorkspaceKind,
    pub color_theme: theme::ColorTheme,
}

pub fn path_title(path: &Path) -> String {
    if path.file_name().and_then(|s| s.to_str()) == Some("project.zipzi.json") {
        if let Some(parent) = path.parent().and_then(|p| p.file_name()) {
            return parent.to_string_lossy().into_owned();
        }
    }
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn format_parent_path(path: &Path) -> String {
    let parent = if path.file_name().and_then(|s| s.to_str()) == Some("project.zipzi.json") {
        path.parent().and_then(|p| p.parent())
    } else {
        path.parent()
    };
    let Some(parent) = parent else {
        return String::new();
    };
    let parent_str = parent.to_string_lossy();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let home_str = home.to_string_lossy();
        if parent_str.starts_with(&*home_str) {
            return format!("~{}", &parent_str[home_str.len()..]);
        }
    }
    parent_str.into_owned()
}

pub fn recent_file_label(path: &Path, all_paths: &[PathBuf]) -> String {
    let title = path_title(path);
    let duplicates = all_paths.iter().filter(|p| path_title(p) == title).count();
    if duplicates > 1 {
        let parent_display = format_parent_path(path);
        if !parent_display.is_empty() {
            format!("{title} — {parent_display}")
        } else {
            title
        }
    } else {
        title
    }
}
/// View controls the canvas and presentation; Window owns navigation and layout recovery.
fn view_menu(item: impl Fn(CommandId) -> MenuItem, selected_theme: theme::ColorTheme) -> Menu {
    Menu::new(t!("menu.view")).items([
        item(CommandId::ModelingWorkspace),
        item(CommandId::AnimationWorkspace),
        item(CommandId::PlayPause),
        item(CommandId::RewindAnimation),
        item(CommandId::ClearExpression),
        item(CommandId::ResetAnimation),
        MenuItem::separator(),
        item(CommandId::ZoomIn),
        item(CommandId::ZoomOut),
        item(CommandId::ResetView),
        item(CommandId::FitView),
        MenuItem::separator(),
        item(CommandId::SelectTool),
        item(CommandId::LassoTool),
        item(CommandId::GlueTool),
        item(CommandId::HandTool),
        item(CommandId::GenerateMesh),
        MenuItem::separator(),
        item(CommandId::ToggleAncestorDeformers),
        item(CommandId::ToggleDeformers),
        item(CommandId::CancelGesture),
        MenuItem::separator(),
        MenuItem::submenu(
            Menu::new(t!("editor.color_theme")).items(theme::ColorTheme::ALL.map(|choice| {
                MenuItem::action(choice.label(), SetColorTheme { theme: choice })
                    .checked(choice == selected_theme)
            })),
        ),
        MenuItem::action(t!("menu.toggle_theme"), ToggleTheme),
        MenuItem::separator(),
        item(CommandId::ShowShortcuts),
        item(CommandId::ResetShortcuts),
    ])
}

impl EditorView {
    pub(super) fn refresh_menus(
        &mut self,
        modal: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.has_focused_input(window, cx);
        let property_form_edit = self.dock.as_ref().is_some_and(|dock| {
            dock.properties
                .read(cx)
                .active_property_edit(&self.property_forms, window, cx)
        });
        let property_state = |id, mut state: zipzi_editor_core::CommandState| {
            if property_form_edit
                && matches!(id, CommandId::CommitProperty | CommandId::CancelProperty)
            {
                state.enabled = true;
                state.disabled_reason = None;
            }
            state
        };
        let settings = cx.global::<settings::Settings>();
        let state = MenuState {
            color_theme: theme::current(cx),
            workspace: self.editor.workspace(),
            deformer_visibility: self.editor.deformer_visibility(),
            commands: COMMANDS
                .iter()
                .map(|s| {
                    property_state(
                        s.id,
                        self.editor.command_state(
                            self.viewport_command(
                                s.id,
                                zipzi_editor_core::CommandSource::NativeMenu,
                            ),
                        ),
                    )
                })
                .collect(),
            creation_reasons: crate::authoring::creation_commands()
                .map(|id| {
                    let expected = if crate::authoring::animation::creation_kind(id).is_some() {
                        zipzi_editor_core::EditingContext::Animation
                    } else {
                        zipzi_editor_core::EditingContext::Modeling
                    };
                    if self.editor.is_busy() || self.editor.editing_context() != Some(expected) {
                        return None;
                    }
                    match self.editor.authoring_command_capability(id) {
                        Ok(zipzi_sdk::CapabilityState::Available) => None,
                        Ok(
                            zipzi_sdk::CapabilityState::Unavailable { code }
                            | zipzi_sdk::CapabilityState::Unsupported { code },
                        ) => Some(crate::authoring::availability_reason(code)),
                        Err(error) => {
                            Some(crate::authoring::scene_creation::creation_error(&error))
                        }
                    }
                })
                .collect(),
            targets: COMMANDS
                .iter()
                .map(|spec| {
                    self.viewport_command(spec.id, zipzi_editor_core::CommandSource::NativeMenu)
                        .target
                })
                .collect(),
            text,
            modal,
            recent_files: settings.recent_files.paths.clone(),
            language: settings.language.clone(),
        };
        if self.menu_state == state {
            return;
        }
        self.menu_state = state;
        let recent_files = &settings.recent_files.paths;
        let recent_menu = if recent_files.is_empty() {
            Menu::new(t!("menu.open_recent"))
                .disabled(true)
                .items([MenuItem::action(t!("menu.no_recent_files"), ClearRecent).disabled(true)])
        } else {
            let mut items: Vec<MenuItem> = recent_files
                .iter()
                .map(|path| {
                    let label = recent_file_label(path, recent_files);
                    MenuItem::action(label, OpenRecent { path: path.clone() }).disabled(modal)
                })
                .collect();
            items.push(MenuItem::separator());
            items.push(MenuItem::action(t!("menu.clear_recent"), ClearRecent).disabled(modal));
            Menu::new(t!("menu.open_recent"))
                .disabled(modal)
                .items(items)
        };
        let item = |id: CommandId| {
            let command = self.viewport_command(id, zipzi_editor_core::CommandSource::NativeMenu);
            let state = property_state(id, self.editor.command_state(command));
            let mut label = t!(id.spec().label).into_owned();
            if !state.enabled {
                if let Some(reason) = crate::authoring::creation_commands()
                    .position(|command| command == id)
                    .and_then(|i| self.menu_state.creation_reasons[i].as_ref())
                {
                    label = format!("{label} · {reason}");
                }
            }
            MenuItem::action(label, RunCommand::targeted(command))
                .disabled(modal || !state.enabled)
                .checked(state.checked)
        };
        let edit = if text {
            vec![
                MenuItem::os_action(t!("menu.undo"), input::Undo, OsAction::Undo),
                MenuItem::os_action(t!("menu.redo"), input::Redo, OsAction::Redo),
            ]
        } else {
            vec![item(CommandId::Undo), item(CommandId::Redo)]
        };
        cx.set_menus([
            Menu::new(t!("app.title")).items([
                MenuItem::action(t!("app.about"), About),
                MenuItem::submenu(
                    Menu::new(t!("menu.language")).items([
                        MenuItem::action(t!("menu.follow_system"), SetLanguage { language: None })
                            .checked(settings.language.is_none()),
                        MenuItem::action(
                            t!("menu.english"),
                            SetLanguage {
                                language: Some("en".into()),
                            },
                        )
                        .checked(settings.language.as_deref() == Some("en")),
                        MenuItem::action(
                            t!("menu.simplified_chinese"),
                            SetLanguage {
                                language: Some("zh-CN".into()),
                            },
                        )
                        .checked(settings.language.as_deref() == Some("zh-CN")),
                    ]),
                ),
                MenuItem::separator(),
                MenuItem::os_submenu(t!("menu.services"), SystemMenuType::Services),
                MenuItem::action(t!("app.hide"), Hide),
                MenuItem::action(t!("app.hide_others"), HideOthers),
                MenuItem::action(t!("app.show_all"), ShowAll),
                MenuItem::separator(),
                item(CommandId::Quit),
            ]),
            Menu::new(t!("menu.file")).items([
                item(CommandId::New),
                item(CommandId::Open),
                MenuItem::submenu(recent_menu),
                MenuItem::submenu(Menu::new(t!("menu.import")).items([
                    item(CommandId::ImportModel3),
                    item(CommandId::ImportPsd),
                    item(CommandId::ImportCubism),
                ])),
                MenuItem::separator(),
                item(CommandId::Save),
                item(CommandId::SaveAs),
                MenuItem::submenu(Menu::new(t!("menu.export")).items([
                    item(CommandId::ExportRuntime),
                    item(CommandId::ExportCubism),
                ])),
                item(CommandId::Close),
                MenuItem::action(t!("menu.close_window"), CloseWindow).disabled(modal),
            ]),
            Menu::new(t!("menu.edit")).items(
                edit.into_iter().chain([
                    MenuItem::separator(),
                    MenuItem::os_action(t!("menu.cut"), input::Cut, OsAction::Cut),
                    MenuItem::os_action(t!("menu.copy"), input::Copy, OsAction::Copy),
                    MenuItem::os_action(t!("menu.paste"), input::Paste, OsAction::Paste),
                    MenuItem::os_action(
                        t!("menu.select_all"),
                        input::SelectAll,
                        OsAction::SelectAll,
                    ),
                    item(CommandId::Deselect),
                    item(CommandId::CommitProperty),
                    item(CommandId::CancelProperty),
                    MenuItem::separator(),
                    MenuItem::submenu(
                        Menu::new(t!("menu.create_object")).items(
                            crate::authoring::scene_creation::COMMANDS
                                .into_iter()
                                .map(&item)
                                .chain([
                                    MenuItem::submenu(
                                        Menu::new(t!("authoring.deformation_creation")).items(
                                            crate::authoring::scene_creation::DEFORMATION_COMMANDS
                                                .into_iter()
                                                .map(&item),
                                        ),
                                    ),
                                    MenuItem::separator(),
                                    item(CommandId::CreateParameter),
                                    item(CommandId::CreateParameterGroup),
                                    MenuItem::separator(),
                                    item(CommandId::CreatePhysicsGroup),
                                    item(CommandId::CreatePoseGroup),
                                    item(CommandId::CreatePoseEntry),
                                    item(CommandId::CreatePoseLink),
                                    item(CommandId::CreateMotion),
                                    item(CommandId::CreateMotionGroup),
                                    item(CommandId::CreateExpression),
                                    item(CommandId::CreateExpressionEntry),
                                ]),
                        ),
                    ),
                ]),
            ),
            view_menu(item, theme::current(cx)),
            Menu::new(t!("menu.window")).items([
                item(CommandId::ResetLayout),
                item(CommandId::AnimationTimeline),
                MenuItem::separator(),
                item(CommandId::Previous),
                item(CommandId::Next),
                MenuItem::separator(),
                MenuItem::action(t!("menu.minimize"), Minimize),
                MenuItem::action(t!("menu.zoom_window"), ZoomWindow),
            ]),
        ]);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn recent_file_label_formatting() {
        use super::{path_title, recent_file_label};
        use std::path::PathBuf;

        let p1 = PathBuf::from("/models/mao/project.zipzi.json");
        let p2 = PathBuf::from("/models/shirousagi.zipzi");
        let all = vec![p1.clone(), p2.clone()];

        assert_eq!(path_title(&p1), "mao");
        assert_eq!(path_title(&p2), "shirousagi.zipzi");

        assert_eq!(recent_file_label(&p1, &all), "mao");
        assert_eq!(recent_file_label(&p2, &all), "shirousagi.zipzi");

        // Disambiguate duplicates with parent path
        let dup1 = PathBuf::from("/work/mao/project.zipzi.json");
        let dup2 = PathBuf::from("/backup/mao/project.zipzi.json");
        let dups = vec![dup1.clone(), dup2.clone()];

        assert_eq!(recent_file_label(&dup1, &dups), "mao — /work");
        assert_eq!(recent_file_label(&dup2, &dups), "mao — /backup");
    }
}
