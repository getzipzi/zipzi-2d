//! Real native Metal composition without opening a window or automating UI.
#![cfg(target_os = "macos")]
use editor_scene::MetalTargetConfig;
use editor_scene::{Document, MetalSceneRenderer};
use gpui_apple::metal_renderer::MetalRenderer;
use gpui_kit::{
    Bounds, ContentMask, DevicePixels, PaintSurface, Quad, ScaledPixels, Scene, SurfaceSource,
    point, size,
};
use metal::MTLPixelFormat;

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
        let frame = kasane.draw(
            &gpu.queue,
            config,
            Document {
                opacity: 0.5,
                ..Default::default()
            },
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
        let hidden = kasane.draw(
            &gpu.queue,
            config,
            Document {
                visible: false,
                ..Default::default()
            },
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
        let resized = kasane.draw(&gpu.queue, config, Document::default())?;
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
            Document {
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
            let texture = kasane.draw(
                &gpu.queue,
                config,
                Document {
                    opacity: 0.5,
                    ..Default::default()
                },
            )?;
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
