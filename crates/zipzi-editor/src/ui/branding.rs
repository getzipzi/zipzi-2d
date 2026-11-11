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
                    include_bytes!("../../assets/app-icons/zipzi-editor-icon.svg"),
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
                        include_bytes!("../../assets/app-icons/zipzi-editor-icon.svg").as_slice(),
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
                    assert!(
                        pixels.chunks_exact(4).any(|pixel| {
                            (pixel[3] > 0 && pixel[3] < 255)
                                || (pixel[3] == 255
                                    && pixel[..3].iter().all(|channel| *channel >= 240)
                                    && pixel[..3].iter().any(|channel| *channel < 255))
                        }),
                        "brand edges must retain antialiased pixel coverage"
                    );
                }
            }
        });
    }
}
