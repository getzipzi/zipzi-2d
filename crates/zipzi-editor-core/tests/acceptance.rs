//! Licensed import/export and real-model animation acceptance.
//! Kept in the default/full Cargo suite, separately selectable from core iteration.
#[allow(dead_code)]
mod support;

#[allow(dead_code)]
#[path = "cases/animation/acceptance.rs"]
mod animation;
#[path = "cases/mao_canvas.rs"]
mod mao_canvas;
#[path = "cases/psd_import.rs"]
mod psd_import;
#[path = "cases/transfer.rs"]
mod transfer;
#[path = "cases/transfer_roundtrip/mod.rs"]
mod transfer_roundtrip;
