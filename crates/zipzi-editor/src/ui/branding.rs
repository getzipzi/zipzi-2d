//! Shared application mark and theme-aware wordmark for identity surfaces.
use gpui_kit::component::{ActiveTheme, Colorize};
use gpui_kit::{prelude::*, *};
use std::sync::Arc;

use super::compact;

struct BrandImages {
    scale: f32,
    color: String,
    icon: Arc<RenderImage>,
    wordmark: Arc<RenderImage>,
}

fn rasterize(bytes: &[u8], width: f32, height: f32, cx: &App) -> Arc<RenderImage> {
    let renderer = cx.svg_renderer();
    let parsed = renderer.parse_svg(bytes).expect("valid bundled brand SVG");
    renderer
        .render_parsed(
            &parsed,
            SvgSize::ExactSize(size(
                DevicePixels(width.round().max(1.) as i32),
                DevicePixels(height.round().max(1.) as i32),
            )),
        )
        .expect("render bundled brand SVG")
}

pub(super) fn lockup(window: &mut Window, cx: &mut App) -> Div {
    let scale = compact::scale(cx) * window.scale_factor();
    let color = cx.theme().foreground.to_hex();
    let cache = window.use_keyed_state("zipzi-brand-images", cx, |_, _| None::<BrandImages>);
    let (icon, wordmark) = cache.update(cx, |cache, cx| {
        if cache
            .as_ref()
            .is_none_or(|cached| cached.scale != scale || cached.color != color)
        {
            let wordmark =
                include_str!("../../assets/branding/zipzi-wordmark.svg").replace("#000", &color);
            // Rasterize at the actual display pixel size. Downsampling a large
            // icon texture with GPU bilinear filtering aliases curved outlines.
            *cache = Some(BrandImages {
                scale,
                color,
                icon: rasterize(
                    include_bytes!("../../assets/branding/zipzi-logo.svg"),
                    64. * scale,
                    64. * scale,
                    cx,
                ),
                wordmark: rasterize(
                    wordmark.as_bytes(),
                    64. * 720. / 310. * scale,
                    64. * scale,
                    cx,
                ),
            });
        }
        let cached = cache.as_ref().unwrap();
        (cached.icon.clone(), cached.wordmark.clone())
    });
    div()
        .flex()
        .items_center()
        .justify_center()
        .gap_3()
        .child(img(icon).size(compact::rem(64.)))
        .child(
            img(wordmark)
                .id("zipzi-wordmark")
                .test_support()
                .w(compact::rem(64. * 720. / 310.))
                .h(compact::rem(64.)),
        )
}

#[cfg(test)]
mod tests {
    use super::rasterize;
    use gpui_kit::{DevicePixels, TestAppContext};

    #[gpui_kit::test]
    fn welcome_brand_edges_follow_display_resolution(cx: &mut TestAppContext) {
        cx.read(|cx| {
            for scale in [1., 2.] {
                for (bytes, aspect) in [
                    (
                        include_bytes!("../../assets/branding/zipzi-logo.svg").as_slice(),
                        1.,
                    ),
                    (
                        include_bytes!("../../assets/branding/zipzi-wordmark.svg").as_slice(),
                        720. / 310.,
                    ),
                ] {
                    let image = rasterize(bytes, 64. * aspect * scale, 64. * scale, cx);
                    assert_eq!(image.size(0).height, DevicePixels((64. * scale) as i32));
                    assert_eq!(
                        image.size(0).width,
                        DevicePixels((64. * aspect * scale).round() as i32)
                    );
                    let pixels = image.as_bytes(0).unwrap();
                    assert_eq!(
                        pixels[3], 0,
                        "brand artwork must have a transparent background"
                    );
                    assert!(
                        pixels
                            .chunks_exact(4)
                            .any(|pixel| pixel[3] > 0 && pixel[3] < 255),
                        "brand edges must retain antialiased pixel coverage"
                    );
                    if aspect == 1. {
                        let side = (64. * scale) as usize;
                        let factor = side as f32 / 537.;
                        let inset = (side as f32 - 531.5 * factor) / 2.;
                        for (start, end) in [
                            ((632., 334.), (425., 474.)),
                            ((689., 407.), (522.017_7, 519.934_9)),
                            ((632., 165.), (632., 334.)),
                        ] {
                            for step in 1..10 {
                                let t = step as f32 / 10.;
                                let x = start.0 + (end.0 - start.0) * t;
                                let y = start.1 + (end.1 - start.1) * t;
                                let px = (inset + (916.25 - x) * factor) as usize;
                                let py = ((y - 140.) * factor) as usize;
                                assert_eq!(
                                    pixels[(py * side + px) * 4 + 3],
                                    255,
                                    "fold seams must not expose the background"
                                );
                            }
                        }
                    }
                }
            }
        });
    }
}
