use gpui_kit::Rgba;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

impl Hsv {
    /// Hue remains meaningful at gray; saturation remains meaningful at black.
    pub fn from_rgb(color: Rgba, previous: Self) -> Self {
        let max = color.r.max(color.g).max(color.b);
        let min = color.r.min(color.g).min(color.b);
        let delta = max - min;
        let h = if delta == 0. {
            previous.h
        } else if max == color.r {
            ((color.g - color.b) / delta / 6.).rem_euclid(1.)
        } else if max == color.g {
            ((color.b - color.r) / delta + 2.) / 6.
        } else {
            ((color.r - color.g) / delta + 4.) / 6.
        };
        Self {
            h,
            s: if max == 0. { previous.s } else { delta / max },
            v: max,
        }
    }

    pub fn rgb(self, alpha: f32) -> Rgba {
        let h = self.h.rem_euclid(1.) * 6.;
        let c = self.v * self.s;
        let x = c * (1. - (h.rem_euclid(2.) - 1.).abs());
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.),
            1 => (x, c, 0.),
            2 => (0., c, x),
            3 => (0., x, c),
            4 => (x, 0., c),
            _ => (c, 0., x),
        };
        let m = self.v - c;
        Rgba {
            r: r + m,
            g: g + m,
            b: b + m,
            a: alpha,
        }
    }
}

pub(super) fn hex(color: Rgba, alpha: bool) -> String {
    let channel = |value: f32| (value * 255.).round() as u8;
    let mut text = format!(
        "#{:02X}{:02X}{:02X}",
        channel(color.r),
        channel(color.g),
        channel(color.b)
    );
    if alpha {
        text.push_str(&format!("{:02X}", channel(color.a)));
    }
    text
}

pub(super) fn parse_hex(text: &str, alpha: bool, previous_alpha: f32) -> Option<Rgba> {
    let text = text.trim().trim_start_matches('#');
    if !text.is_ascii() || !text.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let expanded;
    let text = match text.len() {
        3 => {
            expanded = text.chars().flat_map(|c| [c, c]).collect::<String>();
            &expanded
        }
        4 if alpha => {
            expanded = text.chars().flat_map(|c| [c, c]).collect::<String>();
            &expanded
        }
        6 => text,
        8 if alpha => text,
        _ => return None,
    };
    let channel = |offset| {
        u8::from_str_radix(&text[offset..offset + 2], 16)
            .ok()
            .map(|v| v as f32 / 255.)
    };
    Some(Rgba {
        r: channel(0)?,
        g: channel(2)?,
        b: channel(4)?,
        a: if text.len() == 8 {
            channel(6)?
        } else {
            previous_alpha
        },
    })
}

pub(super) fn number(value: f32) -> String {
    format!("{value:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gray_and_black_preserve_the_authored_hue_and_black_saturation() {
        let blue = Hsv {
            h: 0.65,
            s: 0.8,
            v: 0.9,
        };
        let gray = Hsv::from_rgb(
            Rgba {
                r: 0.4,
                g: 0.4,
                b: 0.4,
                a: 1.,
            },
            blue,
        );
        assert_eq!(gray.h, blue.h);
        assert_eq!(gray.s, 0.);
        let black = Hsv::from_rgb(
            Rgba {
                r: 0.,
                g: 0.,
                b: 0.,
                a: 1.,
            },
            blue,
        );
        assert_eq!((black.h, black.s), (blue.h, blue.s));
        let restored = Hsv { v: 0.9, ..black }.rgb(1.);
        let original = blue.rgb(1.);
        assert_eq!(restored, original);
    }

    #[test]
    fn hsv_roundtrips_rgb_and_hex_obeys_the_alpha_mode() {
        for color in [
            Rgba {
                r: 0.13,
                g: 0.67,
                b: 0.91,
                a: 0.3,
            },
            Rgba {
                r: 1.,
                g: 0.,
                b: 0.,
                a: 1.,
            },
        ] {
            let restored = Hsv::from_rgb(color, Hsv::default()).rgb(color.a);
            for (a, b) in [
                (color.r, restored.r),
                (color.g, restored.g),
                (color.b, restored.b),
            ] {
                assert!((a - b).abs() < 0.000001);
            }
        }
        assert_eq!(
            hex(parse_hex("#abc8", true, 1.).unwrap(), true),
            "#AABBCC88"
        );
        assert!(parse_hex("#abc8", false, 1.).is_none());
        assert!(parse_hex("#12", false, 1.).is_none());
        assert_eq!(parse_hex("#abcdef", true, 0.3).unwrap().a, 0.3);
    }
}
