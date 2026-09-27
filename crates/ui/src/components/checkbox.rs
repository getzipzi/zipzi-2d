//! Controlled boolean control; the owner supplies and updates `checked`.

use gpui::*;

use super::button::button;

pub fn checkbox(id: impl Into<ElementId>, label: &str, checked: bool) -> Stateful<Div> {
    button(id, format!("{}  {label}", if checked { "☑" } else { "☐" }))
}
