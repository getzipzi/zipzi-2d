use super::{Harness, manual_mesh::present, setup_with_document};
use gpui_kit::{
    AppContext as _, InputEvent as _, MouseButton, ScrollDelta, ScrollWheelEvent, TestAppContext,
    point, px,
    test::{ClickOptions, TestWindowExt},
};
use zipzi_core::{Canvas, Part, Vec2};
use zipzi_editor_core::{Document, ModelPoint, ObjectKind, ObjectRef};
use zipzi_sdk::{AuthoringSession, prepare_png_asset, rectangle_mesh};

fn id(n: u128) -> String {
    uuid::Uuid::from_u128(n).to_string()
}

fn fixture(cx: &mut TestAppContext, count: usize) -> Harness {
    let assets = tempfile::tempdir().unwrap();
    let texture = assets.path().join("texture.png");
    std::fs::write(
        &texture,
        include_bytes!("../../../fixtures/mesh-generation/opaque32.png"),
    )
    .unwrap();
    let mut session =
        AuthoringSession::new(&id(1), Canvas::new(256., 256., Vec2::default(), 1.)).unwrap();
    let asset = prepare_png_asset(&id(2), "Texture", &texture).unwrap();
    session
        .edit("Overlapping objects", None, |edit| {
            edit.create_asset(asset)?;
            edit.create_part(Part {
                id: id(3),
                name: "Face · details".into(),
                ..Default::default()
            })?;
            for n in 0..count {
                let mut mesh = rectangle_mesh(
                    &id(100 + n as u128),
                    &format!("Mesh {n}"),
                    &id(2),
                    Vec2::new(24., 24.),
                    Vec2::new(232., 232.),
                )?;
                mesh.part_id = id(3);
                edit.create_mesh(mesh)?;
            }
            Ok(())
        })
        .unwrap();
    let saved = session
        .save_project(&assets.path().join("menu.zipzi"), None)
        .unwrap();
    let mut document = Document::open(&saved.path).unwrap();
    document
        .select_object(ObjectRef::new(ObjectKind::Mesh, id(101)))
        .unwrap();
    let h = setup_with_document(cx, document, assets);
    present(&h, cx);
    h
}

fn open(h: &Harness, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        let p = h
            .view
            .read(cx)
            .modeling_frame()
            .unwrap()
            .canvas
            .transform
            .to_viewport(ModelPoint { x: 128., y: 128. });
        window.click_with_options(
            "canvas-body",
            ClickOptions::new()
                .with_offset(point(px(p.x), px(p.y)))
                .with_button(MouseButton::Right),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn canvas_context_menu_keeps_short_object_names_inline(cx: &mut TestAppContext) {
    let h = fixture(cx, 2);
    open(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let labels: Vec<_> = (0..3usize)
            .map(|n| {
                window
                    .within("popup-menu")
                    .find(n)
                    .label()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert!(labels.contains(&"Mesh 0".into()));
        assert!(labels.contains(&"Mesh 1".into()));
        assert!(labels.contains(&"Face · details".into()));
        assert!(window.try_find("submenu").is_none());
        window.press("escape", cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn canvas_context_menu_scrolls_long_candidates_and_keeps_actions_accessible(
    cx: &mut TestAppContext,
) {
    let h = fixture(cx, 48);
    open(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.within("popup-menu").find(0usize).label(),
            Some("Select object")
        );
        let labels: Vec<_> = (0..12usize)
            .filter_map(|n| {
                window
                    .within("popup-menu")
                    .try_find(n)
                    .and_then(|e| e.label().map(str::to_owned))
            })
            .collect();
        assert!(labels.iter().any(|label| label == "Edit original points"));
        assert!(labels.iter().any(|label| label == "Delete Object"));
        let root = window.find("popup-menu").bounds();
        assert!(root.size.height < px(300.));
        window.within("popup-menu").hover(0usize, cx);
        window.render_frame(cx);
        let submenu = window.within("submenu").find("popup-menu").bounds();
        assert!(submenu.size.height < px(400.));
        assert!(submenu.top() >= px(0.));
        assert!(submenu.bottom() <= window.viewport_size().height);
        window.dispatch_event(
            ScrollWheelEvent {
                position: submenu.center(),
                delta: ScrollDelta::Pixels(point(px(0.), px(-3000.))),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        let last = window.within("submenu").within("popup-menu").find(48usize);
        assert_eq!(last.label(), Some("Mesh 0"));
        assert!(last.bounds().top() >= submenu.top());
        assert!(last.bounds().bottom() <= submenu.bottom());
        window
            .within("submenu")
            .within("popup-menu")
            .click(48usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor.document().unwrap().context().selection,
            Some(ObjectRef::new(ObjectKind::Mesh, id(100)))
        );
    });
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("popup-menu").is_none());
        assert_eq!(
            h.view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths()
                .0,
            0
        );
    })
    .unwrap();
}
