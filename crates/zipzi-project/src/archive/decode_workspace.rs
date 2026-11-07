//! Admission estimate for the native document JSON decoder.
//!
//! This is a calibrated heap estimate, not an allocator-enforced hard bound.
//! It includes the input Vec, duplicate-key syntax checking, serde's temporary
//! representations (including flatten/untagged content), typed wire records and
//! conversion to OwnedDocument. ZIP indexes and PNG decoding are charged elsewhere.
//! The reader separately admits retained-model storage before freezing.
//!
//! Coefficients include collection spare capacity and overlapping representations:
//! - strings: four additional bytes per encoded byte for decoded storage, parser
//!   scratch, temporary content and the owned copy; escapes cannot expand UTF-8
//!   beyond their encoded length;
//! - arrays: initial collection allocation; commas charge subsequent elements;
//! - objects: fixed typed-record / initial map-node overhead;
//! - members: keys, map slots, duplicate-key sets and serde content;
//! - elements: numeric/value slots and Vec growth across temporary representations.
//!   On 64-bit builds an opaque numeric array can simultaneously retain serde
//!   content plus the old/new output Vec capacities: roughly five 32-byte slots
//!   per element just above a power of two. Round up to 192 bytes, not 96.
//!
//! These allowances are intentionally rounded, not Rust type-size promises. The
//! tests measure requested live heap bytes with System allocation forwarding over
//! numeric, string, metadata, geometry, keyform, motion and rejected inputs at
//! multiple sizes. Recalibrate when the codec, serde or collection layout changes.
//! They do not measure allocator bookkeeping, RSS, stacks or other threads.
use super::{check, Result};

const STRING_BYTE_ALLOWANCE: usize = 4;
const ARRAY_ALLOWANCE: usize = 128;
const OBJECT_ALLOWANCE: usize = 768;
const ELEMENT_ALLOWANCE: usize = 192;
const MEMBER_ALLOWANCE: usize = 192;

// Frozen roots, record directories, identity indexes and validation scratch have
// a fixed cost even for a nearly empty document. Scaling only the decoded model
// misses that cost (the small calibration fixture peaks at ~6 KiB vs ~4.7 KiB).
const FREEZE_FIXED_ALLOWANCE: usize = 16 << 10;

pub(super) fn document_freeze_workspace(model: &zipzi_core::state::OwnedDocument) -> usize {
    model
        .estimated_bytes()
        .saturating_mul(3)
        .saturating_add(FREEZE_FIXED_ALLOWANCE)
}

#[cfg(test)]
mod tests;

pub(super) fn document_decode_workspace(
    bytes: &[u8],
    keep_going: &mut impl FnMut() -> bool,
) -> Result<usize> {
    // Include the input and additional storage for string decoding and copies.
    // Container and member costs cover collection capacity, growth and temporary
    // serde values. Numeric text only needs the input and its value slots.
    // Scan without allocating before the syntax checker or typed decoder runs.
    let mut workspace = bytes.len();
    let mut in_string = false;
    let mut escaped = false;
    for chunk in bytes.chunks(64 << 10) {
        check(keep_going)?;
        for &byte in chunk {
            if in_string {
                workspace = workspace.saturating_add(STRING_BYTE_ALLOWANCE);
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    in_string = false;
                }
            } else {
                let overhead = match byte {
                    b'"' => {
                        in_string = true;
                        0
                    }
                    b'[' => ARRAY_ALLOWANCE,
                    b'{' => OBJECT_ALLOWANCE,
                    b',' => ELEMENT_ALLOWANCE,
                    b':' => MEMBER_ALLOWANCE,
                    _ => 0,
                };
                workspace = workspace.saturating_add(overhead);
            }
        }
    }
    Ok(workspace)
}
