//! Real native Metal composition without opening a window or automating UI.
#![cfg(target_os = "macos")]
use editor_core::{Document, SceneSnapshot, ViewState};
use editor_render::{MetalSceneRenderer, MetalTargetConfig};
use gpui_apple::metal_renderer::MetalRenderer;
use gpui_kit::{
    Bounds, ContentMask, DevicePixels, PaintSurface, Quad, ScaledPixels, Scene, SurfaceSource,
    point, size,
};
use metal::MTLPixelFormat;
#[path = "../../editor-core/tests/support/mod.rs"]
mod support;

fn input(opacity: f32, visible: bool) -> anyhow::Result<SceneSnapshot> {
    let (mut document, _source) = support::sample_document()?;
    document.preview_opacity(opacity)?;
    document.finish_edit()?;
    document.set_object_visible(&document.context().selection.clone().unwrap(), visible)?;
    document.snapshot()
}

#[test]
fn editor_hidden_mask_source_still_clips_native_metal_output() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        use kasane_core::{Canvas, Vec2};
        let (mut document, directory) = support::sample_document_with_two_layers()?;
        let saved = document.save(&directory.path().join("mask.kasane"))?;
        let mut sdk = kasane_sdk::AuthoringSession::new(
            &uuid::Uuid::new_v4().to_string(),
            Canvas::new(256., 256., Vec2::default(), 1.),
        )?;
        sdk.open_project(&saved.manifest, None)?;
        let ids = sdk.mesh_ids().to_vec();
        let mut source = sdk.mesh(&ids[0]).unwrap();
        let mut masked = sdk.mesh(&ids[1]).unwrap();
        for (mesh, right) in [(&mut source, 128.), (&mut masked, 232.)] {
            mesh.vertex_ids = vec![0, 1, 2, 3];
            mesh.base_positions = vec![
                Vec2::new(24., 24.),
                Vec2::new(right, 24.),
                Vec2::new(right, 232.),
                Vec2::new(24., 232.),
            ];
            mesh.uvs = vec![Vec2::new(0.5, 0.5); 4];
            mesh.triangles = vec![[0, 1, 2], [0, 2, 3]];
        }
        masked.masks = vec![source.id.clone()];
        sdk.edit("Mask fixture", None, |e| {
            e.replace_mesh(source)?;
            e.replace_mesh(masked)
        })?;
        sdk.save_project(&saved.manifest, None)?;
        let mut document = Document::open(&saved.manifest)?;
        document.set_object_visible(
            &editor_core::ObjectRef::new(editor_core::ObjectKind::Mesh, &ids[0]),
            false,
        )?;
        let hidden = document.snapshot()?;
        assert!(hidden.runtime_frame.drawables[0].visible);
        assert!(!hidden.viewport_frame.drawables[0].visible);
        let mut gpui = MetalRenderer::new_headless(Default::default());
        let gpu = gpui.shared_metal_context();
        let config = MetalTargetConfig {
            width: 256,
            height: 256,
            format: MTLPixelFormat::RGBA8Unorm,
        };
        let mut renderer = MetalSceneRenderer::new(gpu.device.clone(), config)?;
        let mut capture = |snapshot: &SceneSnapshot| -> anyhow::Result<image::RgbaImage> {
            let texture = renderer.draw(&gpu.queue, config, snapshot, ViewState::default())?;
            Ok(gpui.render_scene_to_image(
                &scene(texture, 256., false),
                size(DevicePixels(256), DevicePixels(256)),
            )?)
        };
        let clipped = capture(&hidden)?;
        assert_ne!(
            clipped.get_pixel(80, 128).0,
            [255; 4],
            "Hidden source must still contribute mask coverage"
        );
        assert_eq!(
            clipped.get_pixel(180, 128).0,
            [255; 4],
            "Mask must still clip outside its geometry"
        );
        let mut unmasked = hidden;
        std::sync::Arc::make_mut(&mut unmasked.viewport_frame).drawables[1]
            .masks
            .clear();
        assert_ne!(capture(&unmasked)?.get_pixel(180, 128).0, [255; 4]);
        Ok(())
    })
}

#[test]
#[ignore = "requires models/local/heijao/heijiao.psd"]
fn imported_heijiao_renders_with_clipping_groups() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../models/local/heijao/heijiao.psd");
        let (mut document, _) = Document::import_psd(&source)?;
        let mut imported = document.snapshot()?;
        let mut gpui = MetalRenderer::new_headless(Default::default());
        let gpu = gpui.shared_metal_context();
        let config = MetalTargetConfig {
            width: 800,
            height: 1000,
            format: MTLPixelFormat::RGBA8Unorm,
        };
        let mut renderer = MetalSceneRenderer::new(gpu.device.clone(), config)?;
        let mut capture = |snapshot: &SceneSnapshot| -> anyhow::Result<image::RgbaImage> {
            let texture = renderer.draw(&gpu.queue, config, snapshot, ViewState::default())?;
            let mut output = Scene::default();
            let bounds = bounds(0., 0., config.width as f32, config.height as f32);
            output.insert_primitive(quad(bounds, 0xffffff));
            output.insert_primitive(PaintSurface {
                order: 0,
                bounds,
                content_mask: ContentMask { bounds },
                source: SurfaceSource::MetalTexture(texture),
                opacity: 1.0,
            });
            output.finish();
            Ok(gpui.render_scene_to_image(
                &output,
                size(
                    DevicePixels(config.width as i32),
                    DevicePixels(config.height as i32),
                ),
            )?)
        };
        let masked = capture(&imported)?;
        for drawable in &mut std::sync::Arc::make_mut(&mut imported.viewport_frame).drawables {
            drawable.raw_blend_mode = None;
        }
        let unmasked = capture(&imported)?;
        assert_ne!(
            masked, unmasked,
            "Imported clipping must affect rendered pixels"
        );
        assert!(masked.pixels().any(|pixel| pixel.0 != [255; 4]));
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/psd-import");
        std::fs::create_dir_all(&directory)?;
        masked.save(directory.join("heijiao.png"))?;
        Ok(())
    })
}

fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<ScaledPixels> {
    Bounds::new(
        point(ScaledPixels(x), ScaledPixels(y)),
        size(ScaledPixels(w), ScaledPixels(h)),
    )
}
fn quad(bounds: Bounds<ScaledPixels>, color: u32) -> Quad {
    Quad {
        order: 0,
        border_style: Default::default(),
        bounds,
        content_mask: ContentMask { bounds },
        background: gpui_kit::rgb(color).into(),
        border_color: Default::default(),
        corner_radii: Default::default(),
        border_widths: Default::default(),
    }
}
fn scene(texture: metal::Texture, side: f32, clipped: bool) -> Scene {
    let mut scene = Scene::default();
    scene.insert_primitive(quad(bounds(0., 0., side, side), 0xffffff));
    scene.insert_primitive(PaintSurface {
        order: 0,
        bounds: bounds(0., 0., side, side),
        content_mask: ContentMask {
            bounds: if clipped {
                bounds(100., 0., side - 100., side)
            } else {
                bounds(0., 0., side, side)
            },
        },
        source: SurfaceSource::MetalTexture(texture),
        opacity: 0.5,
    });
    if clipped {
        scene.insert_primitive(quad(bounds(120., 120., 16., 16.), 0xff0000));
    }
    scene.finish();
    scene
}
#[test]
fn native_metal_composes_kasane_without_readback_between_renderers() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        let mut gpui = MetalRenderer::new_headless(Default::default());
        let gpu = gpui.shared_metal_context();
        let mut config = MetalTargetConfig {
            width: 256,
            height: 256,
            format: MTLPixelFormat::RGBA8Unorm,
        };
        let mut kasane = MetalSceneRenderer::new(gpu.device.clone(), config)?;
        let mut blank = Document::new_blank()?;
        kasane.draw(&gpu.queue, config, &blank.snapshot()?, ViewState::default())?;
        let (mut document, _source) = support::sample_document()?;
        document.preview_opacity(0.5)?;
        document.finish_edit()?;
        let frame = kasane.draw(
            &gpu.queue,
            config,
            &document.snapshot()?,
            ViewState::default(),
        )?;
        assert!(std::ptr::eq(frame.device(), gpu.device.as_ref()));
        // No producer wait: GPUI uses the same queue and samples the published texture.
        let original = scene(frame, 256., true);
        let image =
            gpui.render_scene_to_image(&original, size(DevicePixels(256), DevicePixels(256)))?;
        assert_eq!(
            image.get_pixel(128, 128).0,
            [255, 0, 0, 255],
            "overlay ordering"
        );
        assert_eq!(
            image.get_pixel(96, 128).0,
            [255, 255, 255, 255],
            "content mask"
        );
        for (actual, expected) in image
            .get_pixel(112, 128)
            .0
            .iter()
            .zip([216i16, 243, 249, 255])
        {
            assert!(
                (*actual as i16 - expected).abs() <= 2,
                "premultiplied color/opacity: {:?}",
                image.get_pixel(112, 128)
            );
        }
        document.set_object_visible(&document.context().selection.clone().unwrap(), false)?;
        let hidden = kasane.draw(
            &gpu.queue,
            config,
            &document.snapshot()?,
            ViewState::default(),
        )?;
        let image = gpui.render_scene_to_image(
            &scene(hidden, 256., false),
            size(DevicePixels(256), DevicePixels(256)),
        )?;
        assert_eq!(image.get_pixel(128, 128).0, [255; 4], "hidden layer");
        // Publishing a new frame must not mutate the frame retained by a prior scene.
        let replay =
            gpui.render_scene_to_image(&original, size(DevicePixels(256), DevicePixels(256)))?;
        for (actual, expected) in replay
            .get_pixel(112, 128)
            .0
            .iter()
            .zip([216i16, 243, 249, 255])
        {
            assert!(
                (*actual as i16 - expected).abs() <= 2,
                "previous frame changed"
            );
        }
        config.width = 128;
        config.height = 128;
        document.undo()?; // Undo visibility, then undo opacity using the same mesh/resource IDs.
        document.undo()?;
        let resized = kasane.draw(
            &gpu.queue,
            config,
            &document.snapshot()?,
            ViewState::default(),
        )?;
        let image = gpui.render_scene_to_image(
            &scene(resized, 128., false),
            size(DevicePixels(128), DevicePixels(128)),
        )?;
        for (actual, expected) in image
            .get_pixel(64, 64)
            .0
            .iter()
            .zip([178i16, 230, 243, 255])
        {
            assert!(
                (*actual as i16 - expected).abs() <= 2,
                "resized native surface: {:?}",
                image.get_pixel(64, 64)
            );
        }
        let full_coverage = image.pixels().filter(|pixel| pixel.0 != [255; 4]).count();
        let smaller = kasane.draw(
            &gpu.queue,
            config,
            &document.snapshot()?,
            ViewState {
                zoom: 0.5,
                ..Default::default()
            },
        )?;
        let image = gpui.render_scene_to_image(
            &scene(smaller, 128., false),
            size(DevicePixels(128), DevicePixels(128)),
        )?;
        let small_coverage = image.pixels().filter(|pixel| pixel.0 != [255; 4]).count();
        assert!(
            small_coverage < full_coverage / 2,
            "zoom must reduce rendered area"
        );
        config.width = 192;
        let wide = kasane.draw(
            &gpu.queue,
            config,
            &document.snapshot()?,
            ViewState::default(),
        )?;
        assert_eq!((wide.width(), wide.height()), (192, 128));
        Ok(())
    })
}

#[test]
fn native_metal_preserves_premultiplied_alpha_for_transparent_targets() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        // A layer-backed renderer with transparency enabled; no window is opened.
        let mut gpui = MetalRenderer::new(Default::default(), true);
        let gpu = gpui.shared_metal_context();
        for format in [MTLPixelFormat::RGBA8Unorm, MTLPixelFormat::BGRA8Unorm] {
            let config = MetalTargetConfig {
                width: 128,
                height: 128,
                format,
            };
            let mut kasane = MetalSceneRenderer::new(gpu.device.clone(), config)?;
            let texture =
                kasane.draw(&gpu.queue, config, &input(0.5, true)?, ViewState::default())?;
            let mut scene = Scene::default();
            scene.insert_primitive(PaintSurface {
                order: 0,
                bounds: bounds(0., 0., 128., 128.),
                content_mask: ContentMask {
                    bounds: bounds(0., 0., 128., 128.),
                },
                source: SurfaceSource::MetalTexture(texture),
                opacity: 0.5,
            });
            scene.finish();
            let image =
                gpui.render_scene_to_image(&scene, size(DevicePixels(128), DevicePixels(128)))?;
            assert_eq!(image.get_pixel(0, 0).0, [0; 4]);
            for (actual, expected) in image.get_pixel(64, 64).0.iter().zip([25i16, 51, 58, 64]) {
                assert!(
                    (*actual as i16 - expected).abs() <= 2,
                    "transparent {format:?}: {:?}",
                    image.get_pixel(64, 64)
                );
            }
        }
        Ok(())
    })
}

#[test]
fn content_cache_isolates_same_asset_ids_and_reuses_textures_across_views() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| {
        let mut gpui = MetalRenderer::new_headless(Default::default());
        let gpu = gpui.shared_metal_context();
        let config = MetalTargetConfig {
            width: 256,
            height: 256,
            format: MTLPixelFormat::RGBA8Unorm,
        };
        let mut renderer = MetalSceneRenderer::new(gpu.device.clone(), config)?;
        let a = input(1., true)?;
        let mut b = a.clone();
        let mut textures = b.textures.to_vec();
        textures[0].cache_key = "blue-fixture".into();
        textures[0].rgba = vec![0, 0, 255, 255]
            .repeat((textures[0].width * textures[0].height) as usize)
            .into();
        b.textures = textures.into();
        let draw = |renderer: &mut MetalSceneRenderer,
                    gpui: &mut MetalRenderer,
                    frame: &SceneSnapshot|
         -> anyhow::Result<[u8; 4]> {
            let image = renderer.draw(&gpu.queue, config, frame, ViewState::default())?;
            Ok(gpui
                .render_scene_to_image(
                    &scene(image, 256., false),
                    size(DevicePixels(256), DevicePixels(256)),
                )?
                .get_pixel(128, 128)
                .0)
        };
        let original = draw(&mut renderer, &mut gpui, &a)?;
        assert_eq!(renderer.stats().1, 1);
        let blue = draw(&mut renderer, &mut gpui, &b)?;
        assert_ne!(original, blue);
        assert_eq!(renderer.stats().1, 2);
        assert_eq!(draw(&mut renderer, &mut gpui, &a)?, original);
        assert_eq!(renderer.stats().1, 2);
        renderer.draw(
            &gpu.queue,
            config,
            &a,
            ViewState {
                zoom: 1.5,
                ..Default::default()
            },
        )?;
        assert_eq!(
            renderer.stats().1,
            2,
            "viewport changes do not upload textures"
        );
        let mut alias = a.clone();
        let mut frame = (*alias.viewport_frame).clone();
        frame.drawables[0].texture_asset_id = "other-document-asset".into();
        alias.viewport_frame = std::sync::Arc::new(frame);
        let mut textures = alias.textures.to_vec();
        textures[0].asset_id = "other-document-asset".into();
        alias.textures = textures.into();
        assert_eq!(draw(&mut renderer, &mut gpui, &alias)?, original);
        assert_eq!(
            renderer.stats().1,
            2,
            "identical content shares immutable texture storage"
        );
        Ok(())
    })
}

#[test]
fn mesh_candidate_preview_renders_only_its_triangles() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        use editor_core::{MeshGenerationOptions, MeshGridOptions};
        let (doc, _dir) = support::rectangle_document(false)?;
        let source = doc.mesh_generation_source(&doc.mesh_generation_target()?)?;
        let plan = source.generate(MeshGenerationOptions::Grid(MeshGridOptions {
            columns: 1,
            rows: 1,
        }))?;
        let mut texture = source.texture.unwrap();
        // Opaque color makes missing geometry observable without a wireframe.
        texture.rgba = [230, 45, 80, 255]
            .repeat((texture.width * texture.height) as usize)
            .into();
        texture.cache_key = "opaque-candidate-test".into();
        let full = plan.scene(texture);
        let mut missing = full.clone();
        std::sync::Arc::make_mut(&mut missing.viewport_frame).drawables[0].indices =
            full.viewport_frame.drawables[0].indices[..3]
                .to_vec()
                .into();
        let mut gpui = MetalRenderer::new_headless(Default::default());
        let gpu = gpui.shared_metal_context();
        for size_px in [32, 208, 416] {
            let config = MetalTargetConfig {
                width: size_px,
                height: size_px,
                format: MTLPixelFormat::RGBA8Unorm,
            };
            let mut renderer = MetalSceneRenderer::new(gpu.device.clone(), config)?;
            let mut capture = |snapshot: &SceneSnapshot| -> anyhow::Result<image::RgbaImage> {
                let texture = renderer.draw(&gpu.queue, config, snapshot, ViewState::default())?;
                Ok(gpui.render_scene_to_image(
                    &scene(texture, size_px as f32, false),
                    size(DevicePixels(size_px as i32), DevicePixels(size_px as i32)),
                )?)
            };
            let whole = capture(&full)?;
            assert_eq!(
                whole,
                capture(&full)?,
                "Same rectangle must render repeatably"
            );
            let cut = capture(&missing)?;
            assert_ne!(
                whole, cut,
                "Missing triangle must remain visible with no wireframe"
            );
            let (x, y) = (size_px / 4, 3 * size_px / 4);
            assert_ne!(whole.get_pixel(x, y), cut.get_pixel(x, y));
            assert_eq!(cut.get_pixel(x, y).0, [255; 4]);
            assert_eq!(whole.get_pixel(y, x), cut.get_pixel(y, x));
        }
        Ok(())
    })
}
