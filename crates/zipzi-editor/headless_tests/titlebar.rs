use super::setup;
use gpui_kit::{
    AppContext as _, BorrowAppContext as _, TestAppContext, point, px, size, test::TestWindowExt,
};
use zipzi_editor_core::CommandId;

#[gpui_kit::test]
fn titlebar_combines_projects_centered_workspace_and_more(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let title = window.find("workspace-titlebar").bounds();
        let project = window.find("project-select").bounds();
        let switcher = window.find("workspace-switcher").bounds();
        let more = window.find("workspace-layout-menu").bounds();
        let drag = window.find("window-titlebar-drag-region").bounds();
        assert_eq!(title.top(), px(0.));
        if cfg!(target_os = "macos") {
            assert!(
                project.left() >= px(80.),
                "project must clear traffic lights"
            );
        } else {
            assert!(project.left() >= window.find("application-menu").bounds().right());
        }
        assert!(title.contains(&project.origin));
        assert!(project.right() <= drag.left());
        assert!(drag.size.width >= px(12.));
        assert!(drag.right() <= switcher.left());
        assert!((switcher.center().x - title.center().x).abs() <= px(0.5));
        assert!((switcher.center().y - project.center().y).abs() <= px(0.5));
        assert!(more.left() >= switcher.right());
        assert!(more.right() <= title.right());
        assert!(title.bottom() <= window.find("dock-area").bounds().top());
        assert!(window.try_find("workspace-top-bar").is_none());
        assert!(window.try_find("project-new-or-open").is_none());

        window.toggle_fullscreen();
        window.render_frame(cx);
        if cfg!(target_os = "macos") {
            assert!(window.find("project-select").bounds().left() < project.left());
        }
        assert!(
            (window.find("workspace-switcher").bounds().center().x - title.center().x).abs()
                <= px(0.5)
        );
        window.toggle_fullscreen();
        window.render_frame(cx);
        assert_eq!(
            window.find("project-select").bounds().left(),
            project.left()
        );
    })
    .unwrap();
}

#[cfg(not(target_os = "macos"))]
#[gpui_kit::test]
fn inline_application_menu_replaces_project_bar_until_dismissed(cx: &mut TestAppContext) {
    use crate::settings::{Settings, UiScale};
    use gpui_kit::component::ThemeMode;

    let harness = setup(cx);
    for width in [800., 1100.] {
        cx.simulate_window_resize(harness.window, size(px(width), px(740.)));
        for scale in UiScale::ALL {
            let original_focus = cx
                .update_window(harness.window, |_, window, cx| {
                    cx.update_global::<Settings, _>(|settings, _| {
                        settings.set_ui_scale(scale).unwrap()
                    });
                    crate::theme::change(ThemeMode::Dark, cx);
                    window.refresh();
                    window.render_frame(cx);
                    let project = window.find("project-select").bounds();
                    let switcher = window.find("workspace-switcher").bounds();
                    window.click("application-menu", cx);
                    assert!(window.try_find("application-menu").is_none());
                    assert!(window.try_find("project-select").is_none());
                    assert!(window.try_find("popup-menu").is_none());
                    let bar = window.find("application-menu-bar").bounds();
                    assert!(
                        bar.right() <= switcher.left(),
                        "scale={scale:?}, width={width}"
                    );
                    assert_eq!(window.find("workspace-switcher").bounds(), switcher);
                    let count = cx.get_menus().unwrap().len();
                    let mut previous_right = bar.left();
                    for index in 0..count {
                        let entry = window.find(("application-menu-entry", index)).bounds();
                        assert!(entry.left() >= previous_right);
                        assert_eq!(entry.center().y, bar.center().y);
                        previous_right = entry.right();
                    }
                    window.press("escape", cx);
                    assert!(window.try_find("application-menu-items").is_none());
                    assert_eq!(window.find("project-select").bounds(), project);

                    window.click("application-menu", cx);
                    window.click("workspace-modeling", cx);
                    assert!(window.try_find("application-menu-items").is_none());
                    assert!(window.find("project-select").visible());

                    let original_focus = window.focused(cx);
                    window.click("application-menu", cx);
                    for index in [1usize, 2] {
                        window.hover(("application-menu-entry", index), cx);
                        window.render_frame(cx);
                        assert!(
                            window
                                .within(("application-menu-item", index))
                                .find("popup-menu")
                                .visible()
                        );
                    }
                    assert!(window.try_find("project-select").is_none());
                    window.press("escape", cx);
                    original_focus
                })
                .unwrap();
            cx.run_until_parked();
            cx.update_window(harness.window, |_, window, cx| {
                window.render_frame(cx);
                assert!(window.try_find("popup-menu").is_none());
                assert!(window.find("project-select").visible());
                assert_eq!(window.focused(cx), original_focus);

                window.click("application-menu", cx);
                window.hover(("application-menu-entry", 1usize), cx);
                window.click("workspace-modeling", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(harness.window, |_, window, cx| {
                window.render_frame(cx);
                assert!(window.try_find("popup-menu").is_none());
                assert!(window.try_find("application-menu-items").is_none());
                assert!(window.find("application-menu").visible());
                assert!(window.find("project-select").visible());
            })
            .unwrap();
        }
    }
}

#[cfg(not(target_os = "macos"))]
#[gpui_kit::test]
fn application_menu_view_shows_shortcuts_without_scrolling(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let height = px(420.);
    cx.simulate_window_resize(harness.window, size(px(800.), height));
    cx.update_window(harness.window, |_, window, cx| {
        let menus = cx.get_menus().unwrap();
        let view = menus.iter().position(|menu| menu.name == "View").unwrap();
        let color_theme = menus[view]
            .items
            .iter()
            .position(|item| {
                matches!(item, gpui_kit::OwnedMenuItem::Submenu(menu) if menu.name == "Color Theme")
            })
            .unwrap();
        let shortcuts = ["Keyboard Shortcuts", "Reset Shortcuts"].map(|label| {
            menus[view]
                .items
                .iter()
                .position(|item| {
                    matches!(item, gpui_kit::OwnedMenuItem::Action { name, .. } if name == label)
                })
                .unwrap()
        });

        window.click("application-menu", cx);
        window.hover(("application-menu-entry", view), cx);
        window.render_frame(cx);
        let popup = window.find("popup-menu").bounds();
        assert!(popup.top() >= px(0.) && popup.bottom() <= height);
        let menu = window.within("popup-menu");
        assert!(menu.find(color_theme).visible());
        for index in shortcuts {
            let item = menu.find(index);
            assert!(item.visible());
            assert!(item.bounds().bottom() <= popup.bottom());
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn project_menu_switches_closes_and_does_not_trigger_window_drag(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (first, second) = cx
        .update_window(harness.window, |_, window, cx| {
            let first = harness.view.read(cx).editor.active_id().unwrap();
            harness
                .view
                .update(cx, |view, cx| view.run_command(CommandId::New, window, cx));
            (first, harness.view.read(cx).editor.active_id().unwrap())
        })
        .unwrap();
    for id in [first, second] {
        cx.update_window(harness.window, |_, window, cx| {
            window.click("project-select", cx);
            window.click(format!("project-select-{id:?}"), cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(
            harness
                .view
                .read_with(cx, |view, _| view.editor.active_id()),
            Some(id)
        );
    }
    cx.update_window(harness.window, |_, window, cx| {
        window.click("project-select", cx);
        window.click("project-close", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, _, cx| {
        assert_eq!(harness.view.read(cx).editor.document_count(), 1);
        assert_eq!(harness.view.read(cx).editor.active_id(), Some(first));
        let runtime = harness.view.read(cx).editor.runtime_host();
        assert_eq!(
            runtime
                .read()
                .session_catalog(runtime.client_id())
                .unwrap()
                .len(),
            1
        );
        assert!(!harness.view.read(cx).editor.exiting());
    })
    .unwrap();
    for id in [
        "project-select",
        "workspace-modeling",
        "workspace-layout-menu",
    ] {
        cx.update_window(harness.window, |_, window, cx| {
            // TestWindow panics if a control arms TitleBar's window drag handler.
            let from = window.find(id).bounds().center();
            window.drag(from, from + point(px(20.), px(0.)), cx);
            window.press("escape", cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn many_projects_fit_at_minimum_width_and_every_ui_scale(cx: &mut TestAppContext) {
    use crate::settings::{Settings, UiScale};
    use gpui_kit::component::ThemeMode;
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            for _ in 0..16 {
                view.editor.new_document().unwrap();
            }
            view.refresh_document(window, cx);
        });
    })
    .unwrap();
    for width in [800., 1100.] {
        cx.simulate_window_resize(harness.window, size(px(width), px(740.)));
        for scale in UiScale::ALL {
            cx.update_window(harness.window, |_, window, cx| {
                cx.update_global::<Settings, _>(|settings, _| {
                    settings.set_ui_scale(scale).unwrap()
                });
                crate::theme::change(ThemeMode::Dark, cx);
                window.refresh();
                window.render_frame(cx);
                let title = window.find("workspace-titlebar").bounds();
                let drag = window.find("window-titlebar-drag-region").bounds();
                let project = window.find("project-select").bounds();
                let switcher = window.find("workspace-switcher").bounds();
                let more = window.find("workspace-layout-menu").bounds();
                assert!(window.find("project-select").visible());
                assert!(project.size.width >= px(100.));
                assert!(
                    project.right() <= drag.left(),
                    "project={project:?}, drag={drag:?}, scale={scale:?}, width={width}"
                );
                assert!(drag.size.width + px(0.5) >= px(12. * scale.factor()));
                assert!(drag.right() <= switcher.left());
                assert!((switcher.center().x - title.center().x).abs() <= px(0.5));
                assert!(more.left() >= switcher.right());
                assert!(more.right() <= title.right());
                assert!(switcher.top() >= title.top());
                assert!(switcher.bottom() <= title.bottom());
                assert_eq!(title.size.width, px(width));
            })
            .unwrap();
        }
    }
}

#[gpui_kit::test]
fn project_select_creates_and_opens_from_an_empty_window(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::Close, window, cx)
        });
        window.render_frame(cx);
        assert_eq!(harness.view.read(cx).editor.document_count(), 0);
        assert!(window.find("project-select").visible());
        assert_ne!(window.find("project-select").disabled(), Some(true));
        assert!(window.try_find("project-select-modified").is_none());
        window.click("project-select", cx);
        assert!(window.try_find("project-close").is_none());
        window.click("project-new", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        assert_eq!(harness.view.read(cx).editor.document_count(), 1);
        window.click("project-select", cx);
        window.click("project-close", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(harness.view.read(cx).editor.document_count(), 0);
        window.click("project-select", cx);
        window.click("project-open", cx);
    })
    .unwrap();
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("project-select").visible());
        assert!(!harness.view.read(cx).modal);
        assert_eq!(harness.view.read(cx).editor.document_count(), 0);
    })
    .unwrap();
}

#[gpui_kit::test]
fn project_select_opens_and_clears_recent_projects(cx: &mut TestAppContext) {
    use crate::settings::Settings;

    let harness = setup(cx);
    let project = harness._config_dir.path().join("recent.zipzi");
    let mut session = crate::test_fixture::fixture_session(
        &uuid::Uuid::new_v4().to_string(),
        zipzi_core::Canvas::new(42., 42., Default::default(), 1.),
    )
    .unwrap();
    crate::test_fixture::save_fixture(&mut session, &project).unwrap();
    let missing = harness._config_dir.path().join("missing.zipzi");

    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.record_recent_file(project.clone(), cx);
            view.record_recent_file(missing.clone(), cx);
        });
        window.render_frame(cx);
        window.click("project-select", cx);
        assert!(window.try_find("project-recent-0").is_none());
        window.within("popup-menu").hover(4usize, cx);
        window.click("project-recent-0", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        assert!(
            !cx.global::<Settings>()
                .recent_files
                .paths
                .contains(&missing)
        );
        assert!(
            harness
                .view
                .read(cx)
                .edit_error
                .as_ref()
                .unwrap()
                .contains("missing.zipzi")
        );
        window.render_frame(cx);
        window.click("project-select", cx);
        window.within("popup-menu").hover(4usize, cx);
        window.click("project-recent-0", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        assert_eq!(harness.view.read(cx).editor.document_count(), 2);
        window.render_frame(cx);
        window.click("project-select", cx);
        window.within("popup-menu").hover(5usize, cx);
        window.click("project-clear-recent", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        assert!(cx.global::<Settings>().recent_files.is_empty());
        window.render_frame(cx);
        window.click("project-select", cx);
        window.within("popup-menu").hover(5usize, cx);
        assert!(window.try_find("project-recent-0").is_none());
        assert!(window.try_find("project-clear-recent").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn close_modified_project_prompts_and_cancel_preserves_it(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor
                .execute(zipzi_editor_core::EditorCommand::CreateParameter(
                    zipzi_editor_core::NewParameter {
                        name: "Test".into(),
                        runtime_id: "ParamTest".into(),
                        minimum: -1.,
                        maximum: 1.,
                        default_value: 0.,
                        ..Default::default()
                    },
                ))
                .unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
        assert!(harness.view.read(cx).editor.document().unwrap().modified());
        assert!(window.find("project-select-modified").visible());
        window.click("project-select", cx);
        window.click("project-close", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(harness.view.read(cx).modal);
        assert_eq!(harness.view.read(cx).editor.document_count(), 1);
        let runtime = harness.view.read(cx).editor.runtime_host();
        assert_eq!(
            runtime
                .read()
                .session_catalog(runtime.client_id())
                .unwrap()
                .len(),
            1
        );
        assert!(runtime.read().session_catalog(runtime.client_id()).unwrap()[0].dirty);
        window.click("project-select", cx);
        assert!(window.try_find("popup-menu").is_none());
        assert_eq!(harness.view.read(cx).editor.document_count(), 1);
    })
    .unwrap();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!harness.view.read(cx).modal);
        assert!(window.find("project-select-modified").visible());
        assert_eq!(harness.view.read(cx).editor.document_count(), 1);
        assert!(harness.view.read(cx).editor.document().unwrap().modified());
        assert!(!harness.view.read(cx).editor.exiting());
        window.click("project-select", cx);
        window.click("project-close", cx);
    })
    .unwrap();
    cx.simulate_prompt_answer("Discard and Close");
    cx.run_until_parked();
    harness.view.update(cx, |view, _| {
        assert!(!view.modal);
        assert_eq!(view.editor.document_count(), 0);
        assert!({
            let host = view.editor.runtime_host();
            host.read()
                .session_catalog(host.client_id())
                .unwrap()
                .is_empty()
        });
        view.editor.attach_resident_sessions().unwrap();
        assert_eq!(view.editor.document_count(), 0);
        assert!(!view.editor.exiting());
    });
}
