//! Keyform icons use the same SVG grid, icon slot and inherited color as Kit icons.
//! The key or axis midpoint is anchored at (12, 12); operation badges sit lower right.
use gpui_kit::component::Icon;

#[derive(Clone, Copy)]
pub(super) enum KeyformGlyph {
    AddCurrent,
    DeleteCurrent,
    AddEndpoints,
    AddThree,
    DeleteAll,
}

impl From<KeyformGlyph> for Icon {
    fn from(kind: KeyformGlyph) -> Self {
        let svg: &[u8] = match kind {
            KeyformGlyph::AddCurrent => include_bytes!("../../../assets/icons/keyform-add.svg"),
            KeyformGlyph::DeleteCurrent => {
                include_bytes!("../../../assets/icons/keyform-delete.svg")
            }
            KeyformGlyph::AddEndpoints => {
                include_bytes!("../../../assets/icons/keyform-add-two.svg")
            }
            KeyformGlyph::AddThree => include_bytes!("../../../assets/icons/keyform-add-three.svg"),
            KeyformGlyph::DeleteAll => {
                include_bytes!("../../../assets/icons/keyform-delete-all.svg")
            }
        };
        Icon::default().data(svg)
    }
}
