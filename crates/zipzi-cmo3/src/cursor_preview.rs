//! Cubism stores a signed single-precision fraction and displays percentages.

pub(crate) fn percent(weight: f32) -> f64 {
    // Scale the decimal value that round-trips to the Cubism float. This keeps
    // values such as 57% exact in Zipzi's property form.
    format!("{}e2", weight.abs())
        .parse()
        .expect("finite cursor weight")
}

pub(crate) fn weight(influence_percent: f64, reversed: bool) -> f32 {
    let magnitude: f32 = format!("{}e-2", influence_percent.abs())
        .parse()
        .expect("finite cursor influence");
    if reversed { -magnitude } else { magnitude }
}
