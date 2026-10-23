//! Real native Metal composition without opening a window or automating UI.
#![cfg(target_os = "macos")]
use editor_core::{Document, SceneSnapshot, ViewState, ViewportPoint};
use editor_render::{MetalSceneRenderer, MetalTargetConfig};
use gpui_apple::metal_renderer::MetalRenderer;
use gpui_kit::{
    Bounds, ContentMask, DevicePixels, PaintSurface, Quad, ScaledPixels, Scene, SurfaceSource,
    point, size,
};
use metal::MTLPixelFormat;
#[path = "../../../editor-core/tests/support/mod.rs"]
mod support;

// Pixel assertions below use a fixed 256px source region centered at (128, 128).
fn fixture_view(zoom: f32) -> ViewState {
    ViewState {
        zoom,
        pan: ViewportPoint {
            x: -128. * zoom,
            y: 128. * zoom,
        },
    }
}

fn input(opacity: f32, visible: bool) -> anyhow::Result<SceneSnapshot> {
    let (mut document, _source) = support::sample_document()?;
    document.preview_opacity(opacity)?;
    document.finish_edit()?;
    document.set_object_visible(&document.context().selection.clone().unwrap(), visible)?;
    document.snapshot()
}

#[test]
fn alpha_mesh_outside_texture_renders_transparent_in_preview_and_history() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        use editor_core::{AlphaMeshOptions, MeshGenerationOptions, ObjectKind, ObjectRef};
        use kasane_core::{Canvas, Vec2};
        let dir = tempfile::tempdir()?;
        let mut session = kasane_sdk::AuthoringSession::new(
            &uuid::Uuid::new_v4().to_string(),
            Canvas::new(64., 64., Vec2::default(), 1.),
        )?;
        let asset_id = uuid::Uuid::new_v4().to_string();
        let mesh_id = uuid::Uuid::new_v4().to_string();
        let asset = session.prepare_rgba_asset(
            &asset_id,
            "solid",
            32,
            32,
            [255, 0, 0, 255].repeat(32 * 32).into(),
        )?;
        let mesh = kasane_sdk::rectangle_mesh(
            &mesh_id,
            "solid",
            &asset_id,
            Vec2::new(16., 16.),
            Vec2::new(48., 48.),
        )?;
        session.edit("fixture", None, |edit| {
            edit.create_asset(asset)?;
            edit.create_mesh(mesh)
        })?;
        let saved = session.save_project(&dir.path().join("solid.kasane"), None)?;
        let mut doc = Document::open(&saved.manifest)?;
        let source = doc.mesh_generation_source(&ObjectRef::new(ObjectKind::Mesh, mesh_id))?;
        let plan = source.generate(MeshGenerationOptions::Alpha(AlphaMeshOptions {
            outside_spacing: 16.,
            inside_spacing: 16.,
            outside_margin: 8.,
            minimum_margin: 3.,
            inside_margin: 4.,
            clip_to_image: false,
            ..AlphaMeshOptions::deformation_small()
        }))?;
        let preview = plan.scene(source.texture.unwrap());
        let mut frames = vec![preview];
        doc.apply_mesh_generation(&plan)?;
        frames.push(doc.snapshot()?);
        doc.undo()?;
        frames.push(doc.snapshot()?);
        doc.redo()?;
        frames.push(doc.snapshot()?);
        let mut gpui = MetalRenderer::new_headless(Default::default());
        let gpu = gpui.shared_metal_context();
        let config = MetalTargetConfig {
            width: 64,
            height: 64,
            format: MTLPixelFormat::RGBA8Unorm,
        };
        let mut renderer = MetalSceneRenderer::new(gpu.device.clone(), config)?;
        for (index, snapshot) in frames.iter().enumerate() {
            let [minimum, maximum] = plan.source_bounds();
            let center = if index == 0 {
                [(maximum.x - minimum.x) / 2., -(maximum.y - minimum.y) / 2.]
            } else {
                [32., 32.]
            };
            let texture = renderer.draw(
                &gpu.queue,
                config,
                snapshot,
                ViewState {
                    zoom: 1.,
                    pan: ViewportPoint {
                        x: -center[0],
                        y: center[1],
                    },
                },
            )?;
            let pixels = gpui.render_scene_to_image(
                &scene(texture, 64., false),
                size(DevicePixels(64), DevicePixels(64)),
            )?;
            for (x, y) in [(12, 32), (52, 32), (32, 12), (32, 52)] {
                assert_eq!(
                    pixels.get_pixel(x, y).0,
                    [255; 4],
                    "edge color bled into transparent mesh margin"
                );
            }
            for (x, y) in [(20, 32), (44, 32), (32, 20), (32, 44)] {
                // The shared test compositor draws the surface at 50% opacity.
                assert_eq!(
                    pixels.get_pixel(x, y).0,
                    [255, 128, 128, 255],
                    "artwork moved or lost coverage"
                );
            }
        }
        Ok(())
    })
}

#[test]
fn canvas_zoom_renders_actual_pixels_at_multiple_viewport_sizes_and_display_scales()
-> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        use editor_core::{SceneTexture, ViewportMetrics, ViewportTransform};

        let (mut document, _source) = support::rectangle_document(false)?;
        let mut snapshot = document.snapshot()?;
        snapshot.textures = snapshot
            .textures
            .iter()
            .map(|texture| SceneTexture {
                asset_id: texture.asset_id.clone(),
                cache_key: "solid-zoom-fixture".into(),
                width: 1,
                height: 1,
                rgba: vec![255, 0, 0, 255].into(),
            })
            .collect::<Vec<_>>()
            .into();
        let mut gpui = MetalRenderer::new_headless(Default::default());
        let gpu = gpui.shared_metal_context();
        let mut renderer = MetalSceneRenderer::new(
            gpu.device.clone(),
            MetalTargetConfig {
                width: 256,
                height: 256,
                format: MTLPixelFormat::RGBA8Unorm,
            },
        )?;
        for side in [256, 640] {
            for dpi in [1., 2.] {
                for zoom in [0.5, 1.] {
                    let config = MetalTargetConfig {
                        width: side,
                        height: side,
                        format: MTLPixelFormat::RGBA8Unorm,
                    };
                    let transform = ViewportTransform::new(
                        ViewportMetrics {
                            width: side as f32 / dpi,
                            height: side as f32 / dpi,
                            scale_factor: dpi,
                        },
                        ViewState {
                            zoom,
                            pan: ViewportPoint {
                                x: -128. * zoom,
                                y: 128. * zoom,
                            },
                        },
                    )
                    .unwrap();
                    let texture =
                        renderer.draw_transformed(&gpu.queue, config, &snapshot, transform)?;
                    let image = gpui.render_scene_to_image(
                        &scene(texture, side as f32, false),
                        size(DevicePixels(side as i32), DevicePixels(side as i32)),
                    )?;
                    let coverage = (0..side)
                        .filter(|x| image.get_pixel(*x, side / 2).0 != [255; 4])
                        .count();
                    assert_eq!(
                        coverage,
                        (208. * zoom * dpi).min(side as f32) as usize,
                        "viewport={side}, dpi={dpi}, zoom={zoom}"
                    );
                }
            }
        }
        Ok(())
    })
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
            let texture = renderer.draw(&gpu.queue, config, snapshot, fixture_view(1.))?;
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
        kasane.draw(&gpu.queue, config, &blank.snapshot()?, fixture_view(1.))?;
        let (mut document, _source) = support::sample_document()?;
        document.preview_opacity(0.5)?;
        document.finish_edit()?;
        let frame = kasane.draw(&gpu.queue, config, &document.snapshot()?, fixture_view(1.))?;
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
        let hidden = kasane.draw(&gpu.queue, config, &document.snapshot()?, fixture_view(1.))?;
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
        let resized = kasane.draw(&gpu.queue, config, &document.snapshot()?, fixture_view(1.))?;
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
        let smaller = kasane.draw(&gpu.queue, config, &document.snapshot()?, fixture_view(0.5))?;
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
        let wide = kasane.draw(&gpu.queue, config, &document.snapshot()?, fixture_view(1.))?;
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
            let texture = kasane.draw(&gpu.queue, config, &input(0.5, true)?, fixture_view(1.))?;
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
            let image = renderer.draw(&gpu.queue, config, frame, fixture_view(1.))?;
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
        renderer.draw(&gpu.queue, config, &a, fixture_view(1.5))?;
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
                let [minimum, maximum] = plan.source_bounds();
                let width = maximum.x - minimum.x;
                let height = maximum.y - minimum.y;
                let zoom = (size_px as f32 / width).min(size_px as f32 / height);
                let view = ViewState {
                    zoom,
                    pan: ViewportPoint {
                        x: -width * zoom / 2.,
                        y: -height * zoom / 2.,
                    },
                };
                let texture = renderer.draw(&gpu.queue, config, snapshot, view)?;
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
            let (x, y) = (size_px / 4, size_px / 4);
            assert_ne!(whole.get_pixel(x, y), cut.get_pixel(x, y));
            assert_eq!(cut.get_pixel(x, y).0, [255; 4]);
            assert_eq!(
                whole.get_pixel(size_px - x, size_px - y),
                cut.get_pixel(size_px - x, size_px - y)
            );
        }
        Ok(())
    })
}

/// The host policy matches the SDK observer before masks / Offscreen. Overlap
/// distinguishes it from multiplying the alpha of a flattened model image.
#[test]
fn animation_model_opacity_matches_observer_with_overlap_mask_and_offscreen() -> anyhow::Result<()>
{
    let (document, assets) = support::sample_document_with_scene_objects()?;
    check_animation_opacity(document, assets, false)
}

#[test]
fn local_mao_opacity_and_added_offscreen_match_observer_pixels() -> anyhow::Result<()> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/local/mao/runtime/mao_pro.model3.json");
    if !path.exists() {
        eprintln!("SKIP licensed Mao model: {}", path.display());
        return Ok(());
    }
    let (document, _) = Document::import_model3(&path)?;
    check_animation_opacity(document, tempfile::tempdir()?, true)
}

fn check_animation_opacity(
    mut document: Document,
    assets: tempfile::TempDir,
    actual_model: bool,
) -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        use editor_core::{AnimationEvaluator, Editor, ViewportMetrics, WorkspaceKind};
        use kasane_core::{Canvas, Vec2};
        use kasane_sdk_observe::{
            ModelRoi, ObservationInput, Observer, ObserverConfig, RenderRequest,
        };
        let canvas = document
            .source_canvas()
            .expect("fixture has a source rectangle");
        let manifest = document
            .save(&assets.path().join("opacity.kasane"))?
            .manifest;
        let mut sdk = kasane_sdk::AuthoringSession::new(
            &uuid::Uuid::new_v4().to_string(),
            Canvas::new(256., 256., Vec2::default(), 1.),
        )?;
        sdk.open_project(&manifest, None)?;
        if actual_model {
            // The bundled Cubism 4 assets have no native Offscreens. Exercise a
            // controlled Kasane Offscreen on real textured, masked geometry.
            let part_id = sdk
                .document()
                .part_order()
                .iter()
                .find(|id| sdk.document().get_part(id).unwrap().parent_id.is_empty())
                .unwrap()
                .clone();
            sdk.edit("Actual-model Offscreen", None, |edit| {
                edit.create_offscreen(kasane_core::Offscreen {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: "Acceptance composite".into(),
                    part_id,
                    ..Default::default()
                })
            })?;
        } else {
            let ids = sdk.mesh_ids().to_vec();
            let mut source = sdk.mesh(&ids[0]).unwrap();
            source.deformer_id.clear();
            let mut masked = sdk.mesh(&ids[1]).unwrap();
            masked.masks = vec![ids[0].clone()];
            masked.deformer_id.clear();
            sdk.edit("Masked overlap", None, |edit| {
                edit.replace_mesh(source)?;
                edit.replace_mesh(masked)
            })?;
        }
        let gpui = MetalRenderer::new_headless(Default::default());
        let gpu = gpui.shared_metal_context();
        let config = MetalTargetConfig {
            width: 256,
            height: 256,
            format: MTLPixelFormat::RGBA8Unorm,
        };
        let mut renderer = MetalSceneRenderer::new(gpu.device.clone(), config)?;
        let mut observer = Observer::new(ObserverConfig {
            width: 256,
            height: 256,
            fit_long_side: 256.,
        })?;
        let mut images = Vec::new();
        for opacity in [0., 0.5, 1.] {
            let motion_id = uuid::Uuid::new_v4().to_string();
            let source = serde_json::json!({"Version":3,"Meta":{"Duration":1,"Fps":60,"Loop":false,
                "AreBeziersRestricted":true,"CurveCount":1,"TotalSegmentCount":1,"TotalPointCount":2,"FadeInTime":0,"FadeOutTime":0,"UserDataCount":0,"TotalUserDataSize":0},
                "Curves":[{"Target":"Model","Id":"Opacity","Segments":[0,opacity,0,1,opacity]}]});
            sdk.edit("Opacity motion", None, |edit| {
                edit.import_motion3(
                    &motion_id,
                    &format!("Opacity {opacity}"),
                    &source.to_string(),
                )
                .map(|_| ())
            })?;
            sdk.save_project(&manifest, None)?;
            let mut preview = sdk.motion_preview();
            preview.schedule_motion(&motion_id, 0.)?;
            preview.seek(0.)?;
            assert_eq!(preview.snapshot().model_opacity, opacity);
            let input = ObservationInput::capture_motion_from_snapshot(
                &sdk.read_snapshot(),
                &preview,
                true,
            )?;
            let expected = observer.render(
                &kasane_sdk_observe::ResolvedObservation::capture(input.clone())?,
                RenderRequest {
                    width: 256,
                    height: 256,
                    roi: ModelRoi {
                        x0: 0.,
                        y0: 0.,
                        x1: canvas.width,
                        y1: canvas.height,
                    },
                    padding_model: 0.,
                },
            )?;
            let mut editor = Editor::from_document(Document::open(&manifest)?);
            editor.register_primary_viewport(
                editor_core::ViewportId::new(),
                editor_core::ViewportKind::Animation,
            )?;
            editor.set_workspace(WorkspaceKind::Animation)?;
            editor.set_canvas_metrics(ViewportMetrics {
                width: 256.,
                height: 256.,
                scale_factor: 1.,
            });
            let selection = editor
                .document()
                .unwrap()
                .motion_resources()
                .into_iter()
                .find(|m| m.selection.clip_id == motion_id)
                .unwrap()
                .selection;
            editor.select_motion(editor.active_id().unwrap(), selection)?;
            let mut evaluator = AnimationEvaluator::default();
            let evaluated =
                evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)?;
            let scene = evaluated.scene;
            assert!(!scene.viewport_frame.offscreens.is_empty());
            assert!(
                scene
                    .viewport_frame
                    .drawables
                    .iter()
                    .any(|d| !d.masks.is_empty())
            );
            let mut reference = input.frame().clone();
            reference.source_revision = scene.viewport_frame.source_revision;
            assert_eq!(scene.render_frame()?.as_ref(), &reference);
            let submitted = renderer.submit_transformed(
                &gpu.queue,
                config,
                &scene,
                editor_core::ViewportTransform::new(
                    ViewportMetrics {
                        width: 256.,
                        height: 256.,
                        scale_factor: 1.,
                    },
                    ViewState {
                        zoom: 256. / canvas.width.max(canvas.height),
                        pan: ViewportPoint {
                            x: (-canvas.width / 2.) * 256. / canvas.width.max(canvas.height),
                            y: (canvas.height / 2.) * 256. / canvas.width.max(canvas.height),
                        },
                    },
                )
                .unwrap(),
            )?;
            submitted.command.wait_until_completed();
            let actual = read_preview(&gpu.device, &gpu.queue, &submitted.image);
            let max_difference = actual
                .iter()
                .zip(&expected.rgba)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(
                max_difference <= 1,
                "opacity={opacity}, max channel difference={max_difference}"
            );
            images.push(actual);
        }
        assert!(images[0].iter().all(|v| *v == 0));
        assert!(
            images[1] != images[2],
            "Half and full opacity must change pixels"
        );
        assert!(
            images[1]
                .iter()
                .zip(&images[2])
                .any(|(half, full)| (*half as i16 - (*full as f32 * 0.5).round() as i16).abs() > 2),
            "Fixture must distinguish per-drawable opacity from final image alpha"
        );
        Ok(())
    })
}

fn read_preview(
    device: &metal::DeviceRef,
    queue: &metal::CommandQueueRef,
    source: &metal::TextureRef,
) -> Vec<u8> {
    let descriptor = metal::TextureDescriptor::new();
    descriptor.set_pixel_format(MTLPixelFormat::RGBA8Unorm);
    descriptor.set_width(source.width());
    descriptor.set_height(source.height());
    descriptor.set_storage_mode(metal::MTLStorageMode::Shared);
    let target = device.new_texture(&descriptor);
    let command = queue.new_command_buffer();
    let blit = command.new_blit_command_encoder();
    blit.copy_from_texture(
        source,
        0,
        0,
        metal::MTLOrigin { x: 0, y: 0, z: 0 },
        metal::MTLSize::new(source.width(), source.height(), 1),
        &target,
        0,
        0,
        metal::MTLOrigin { x: 0, y: 0, z: 0 },
    );
    blit.end_encoding();
    command.commit();
    command.wait_until_completed();
    let mut bytes = vec![0; (source.width() * source.height() * 4) as usize];
    target.get_bytes(
        bytes.as_mut_ptr().cast(),
        source.width() * 4,
        metal::MTLRegion::new_2d(0, 0, source.width(), source.height()),
        0,
    );
    bytes
}

#[test]
fn modeling_isolation_dims_or_hides_color_without_changing_mask_coverage() -> anyhow::Result<()> {
    for nested in [false, true] {
        for overlap_opacity in [1., 0.5] {
            check_modeling_isolation_composite(nested, overlap_opacity)?;
        }
    }
    Ok(())
}

fn check_modeling_isolation_composite(nested: bool, overlap_opacity: f32) -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        use editor_core::{
            Editor, EditorCommand, IsolationDisplay, ObjectKind, ObjectRef, ViewportId,
            ViewportKind, ViewportMetrics,
        };
        use kasane_core::{Canvas, Vec2};
        let directory = tempfile::tempdir()?;
        let mut sdk = kasane_sdk::AuthoringSession::new(
            &uuid::Uuid::new_v4().to_string(),
            Canvas::new(64., 64., Vec2::default(), 1.),
        )?;
        let id = |n| uuid::Uuid::from_u128(n).to_string();
        let red = sdk.prepare_rgba_asset(&id(1), "red", 1, 1, vec![255, 0, 0, 255].into())?;
        let blue = sdk.prepare_rgba_asset(&id(2), "blue", 1, 1, vec![0, 0, 255, 255].into())?;
        let mut mask = kasane_sdk::rectangle_mesh(
            &id(3),
            "mask",
            &id(2),
            Vec2::new(8., 8.),
            Vec2::new(32., 56.),
        )?;
        let mut art = kasane_sdk::rectangle_mesh(
            &id(4),
            "art",
            &id(1),
            Vec2::new(8., 8.),
            Vec2::new(48., 56.),
        )?;
        art.masks = vec![id(3)];
        art.draw_order = Some(1.);
        let mut other = kasane_sdk::rectangle_mesh(
            &id(5),
            "other",
            &id(2),
            Vec2::new(48., 8.),
            Vec2::new(60., 56.),
        )?;
        other.draw_order = Some(2.);
        let mut overlap = kasane_sdk::rectangle_mesh(
            &id(6),
            "overlap",
            &id(1),
            Vec2::new(48., 8.),
            Vec2::new(56., 56.),
        )?;
        overlap.draw_order = Some(3.);
        overlap.appearance.opacity = overlap_opacity;
        if nested {
            for mesh in [&mut mask, &mut art, &mut other, &mut overlap] {
                mesh.part_id = id(8);
            }
        }
        sdk.edit("Fixture", None, |edit| {
            if nested {
                edit.create_part(kasane_core::Part {
                    id: id(7),
                    ..Default::default()
                })?;
                edit.create_part(kasane_core::Part {
                    id: id(8),
                    parent_id: id(7),
                    ..Default::default()
                })?;
                for (group, part) in [(9, 7), (10, 8)] {
                    edit.create_offscreen(kasane_core::Offscreen {
                        id: id(group),
                        runtime_id: id(group),
                        part_id: id(part),
                        ..Default::default()
                    })?;
                }
            }
            edit.create_asset(red)?;
            edit.create_asset(blue)?;
            edit.create_mesh(mask)?;
            edit.create_mesh(art)?;
            edit.create_mesh(other)?;
            edit.create_mesh(overlap)
        })?;
        let saved = sdk.save_project(&directory.path().join("isolation.kasane"), None)?;
        let mut editor = Editor::from_document(Document::open(&saved.manifest)?);
        let viewport = ViewportId::new();
        editor.register_primary_viewport(viewport, ViewportKind::Modeling)?;
        let binding = editor.viewport_binding(viewport).unwrap();
        editor.set_viewport_metrics(
            viewport,
            ViewportMetrics {
                width: 64.,
                height: 64.,
                scale_factor: 1.,
            },
        );
        editor.reset_viewport(binding)?;
        editor.pan_viewport(binding, ViewportPoint { x: -32., y: 32. })?;
        editor.execute(EditorCommand::SelectCanvasObjects(vec![ObjectRef::new(
            ObjectKind::Mesh,
            id(4),
        )]))?;
        let gpui = MetalRenderer::new_headless(Default::default());
        let gpu = gpui.shared_metal_context();
        let config = MetalTargetConfig {
            width: 64,
            height: 64,
            format: MTLPixelFormat::RGBA8Unorm,
        };
        let mut renderer = MetalSceneRenderer::new(gpu.device.clone(), config)?;
        for (display, expected_alpha) in [
            (None, 255u8),
            (Some(IsolationDisplay::Dim), 38),
            (Some(IsolationDisplay::Hide), 0),
        ] {
            if let Some(display) = display {
                if !editor.viewport_isolated(binding) {
                    editor.toggle_viewport_isolation(binding)?;
                }
                editor.set_viewport_isolation_display(binding, display)?;
            }
            let preview = editor.preview_for(binding)?;
            assert_eq!(preview.scene.runtime_frame.drawables[0].opacity, 1.);
            assert!(preview.scene.runtime_frame.drawables[0].visible);
            let output = renderer.submit_transformed(
                &gpu.queue,
                config,
                &preview.scene,
                preview.canvas.unwrap().transform,
            )?;
            output.command.wait_until_completed();
            assert_eq!(
                output.command.status(),
                metal::MTLCommandBufferStatus::Completed
            );
            let pixels = read_preview(&gpu.device, &gpu.queue, &output.image);
            let pixel = |x: usize| &pixels[(32 * 64 + x) * 4..][..4];
            assert_eq!(
                pixel(16),
                &[255, 0, 0, 255],
                "mask coverage changed in {display:?}"
            );
            assert_eq!(
                pixel(40),
                &[0, 0, 0, 0],
                "clipped area changed in {display:?}"
            );
            // Fade the completed red-over-blue context once, retaining its
            // authored occlusion even when the front mesh is translucent.
            for channel in 0..4 {
                let expected = match channel {
                    0 => (f32::from(expected_alpha) * overlap_opacity).round() as u8,
                    2 => (f32::from(expected_alpha) * (1. - overlap_opacity)).round() as u8,
                    3 => expected_alpha,
                    _ => 0,
                };
                assert!(
                    pixel(52)[channel].abs_diff(expected) <= 1,
                    "context overlap in {display:?}: {:?}",
                    pixel(52)
                );
            }
            // A single context mesh and the overlap must have equal coverage.
            assert!(pixel(58)[3].abs_diff(expected_alpha) <= 1);
        }
        Ok(())
    })
}
