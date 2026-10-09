//! Native menu structure and state derived from the core command catalog.
use super::*;
use crate::actions::*;
use editor_core::{COMMANDS, CommandId};
use gpui_kit::component::input;
use std::path::{Path, PathBuf};

#[derive(Clone, Default, PartialEq, Eq)]
pub struct MenuState {
    pub commands: Vec<bool>,
    pub text: bool,
    pub modal: bool,
    pub recent_files: Vec<PathBuf>,
    pub language: Option<String>,
    pub deformer_visibility: editor_core::DeformerVisibility,
}

pub fn path_title(path: &Path) -> String {
    if path.file_name().and_then(|s| s.to_str()) == Some("project.kasane.json") {
        if let Some(parent) = path.parent().and_then(|p| p.file_name()) {
            return parent.to_string_lossy().into_owned();
        }
    }
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn format_parent_path(path: &Path) -> String {
    let parent = if path.file_name().and_then(|s| s.to_str()) == Some("project.kasane.json") {
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
/// View controls the canvas and presentation; Window owns panels and layout.
fn view_menu(item: impl Fn(CommandId) -> MenuItem) -> Menu {
    Menu::new(t!("menu.view")).items([
        item(CommandId::ZoomIn),
        item(CommandId::ZoomOut),
        item(CommandId::ResetView),
        MenuItem::separator(),
        item(CommandId::SelectTool),
        item(CommandId::GlueTool),
        item(CommandId::HandTool),
        item(CommandId::GenerateMesh),
        MenuItem::separator(),
        item(CommandId::ToggleAncestorDeformers),
        item(CommandId::ToggleDeformers),
        item(CommandId::CancelGesture),
        MenuItem::separator(),
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
        let settings = cx.global::<settings::Settings>();
        let state = MenuState {
            deformer_visibility: self.editor.deformer_visibility(),
            commands: COMMANDS
                .iter()
                .map(|s| !modal && self.editor.command_available(self.editor.command(s.id)))
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
            MenuItem::action(t!(id.spec().label).into_owned(), RunCommand { id })
                .disabled(modal || !self.editor.command_available(self.editor.command(id)))
                .checked(match id {
                    CommandId::ToggleAncestorDeformers => {
                        self.editor.deformer_visibility().ancestors
                    }
                    CommandId::ToggleDeformers => self.editor.deformer_visibility().all,
                    _ => false,
                })
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
                item(CommandId::ImportModel3),
                item(CommandId::ImportPsd),
                MenuItem::separator(),
                item(CommandId::Save),
                item(CommandId::SaveAs),
                item(CommandId::Close),
                MenuItem::action(t!("menu.close_window"), CloseWindow).disabled(modal),
            ]),
            Menu::new(t!("menu.edit")).items(edit.into_iter().chain([
                MenuItem::separator(),
                MenuItem::os_action(t!("menu.cut"), input::Cut, OsAction::Cut),
                MenuItem::os_action(t!("menu.copy"), input::Copy, OsAction::Copy),
                MenuItem::os_action(t!("menu.paste"), input::Paste, OsAction::Paste),
                MenuItem::os_action(t!("menu.select_all"), input::SelectAll, OsAction::SelectAll),
                item(CommandId::CommitProperty),
                item(CommandId::CancelProperty),
            ])),
            view_menu(item),
            Menu::new(t!("menu.window")).items([
                item(CommandId::MeshTree),
                item(CommandId::Deformers),
                item(CommandId::DrawOrder),
                item(CommandId::Properties),
                item(CommandId::Relations),
                item(CommandId::Parameters),
                item(CommandId::Keyforms),
                item(CommandId::BlendShapes),
                item(CommandId::Glue),
                item(CommandId::Motions),
                item(CommandId::Expressions),
                item(CommandId::Physics),
                item(CommandId::Pose),
                item(CommandId::Resources),
                item(CommandId::ResetLayout),
                item(CommandId::Previous),
                item(CommandId::Next),
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

        let p1 = PathBuf::from("/models/mao/project.kasane.json");
        let p2 = PathBuf::from("/models/shirousagi.kasane");
        let all = vec![p1.clone(), p2.clone()];

        assert_eq!(path_title(&p1), "mao");
        assert_eq!(path_title(&p2), "shirousagi.kasane");

        assert_eq!(recent_file_label(&p1, &all), "mao");
        assert_eq!(recent_file_label(&p2, &all), "shirousagi.kasane");

        // Disambiguate duplicates with parent path
        let dup1 = PathBuf::from("/work/mao/project.kasane.json");
        let dup2 = PathBuf::from("/backup/mao/project.kasane.json");
        let dups = vec![dup1.clone(), dup2.clone()];

        assert_eq!(recent_file_label(&dup1, &dups), "mao — /work");
        assert_eq!(recent_file_label(&dup2, &dups), "mao — /backup");
    }
}
