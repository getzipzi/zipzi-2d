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
//! Current motion records use typed fields and bounded enum payloads. Charge
//! those fields separately; the legacy v22 conversion and opaque JSON still
//! carry the generic allowances.
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
const MOTION_OBJECT_ALLOWANCE: usize = 128;
const MOTION_SLOT_ALLOWANCE: usize = 64;

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Generic,
    Project,
    Version,
    Document,
    AnimationAssets,
    Motions,
    Motion,
    Tracks,
    Track,
    Segments,
    Segment,
    Target,
    Key,
    Curve,
    Point,
    FrameStep,
    Scalar,
}

impl Shape {
    fn field(self, key: &[u8]) -> Self {
        use Shape::*;
        match (self, key) {
            (Project, b"format_version") => Version,
            (Project, b"document") => Document,
            (Document, b"animation_assets") => AnimationAssets,
            (AnimationAssets, b"motions") => Motions,
            (Motion, b"tracks") => Tracks,
            (Track, b"segments") => Segments,
            (Track, b"initial") | (Segment, b"end") => Key,
            (Track, b"target") => Target,
            (Segment, b"curve") => Curve,
            (Segment, b"frame_step") => FrameStep,
            (Curve, b"control1" | b"control2") => Point,
            (
                Motion,
                b"id"
                | b"name"
                | b"duration"
                | b"fps"
                | b"looping"
                | b"restricted_beziers"
                | b"fade_in"
                | b"fade_out"
                | b"opaque_source_content_hash",
            )
            | (Track, b"id" | b"fade_in" | b"fade_out")
            | (Target, b"kind" | b"runtime_id" | b"parameter_id" | b"part_id" | b"category")
            | (Key, b"id" | b"time" | b"value")
            | (Curve, b"kind" | b"preset")
            | (Point, b"time" | b"value")
            | (FrameStep, b"kind" | b"frames") => Scalar,
            _ => Generic,
        }
    }

    fn array_element(self) -> Self {
        match self {
            Self::Motions => Self::Motion,
            Self::Tracks => Self::Track,
            Self::Segments => Self::Segment,
            _ => Self::Generic,
        }
    }

    fn motion_object(self) -> bool {
        matches!(
            self,
            Self::Motion
                | Self::Track
                | Self::Segment
                | Self::Target
                | Self::Key
                | Self::Curve
                | Self::Point
                | Self::FrameStep
        )
    }
}

#[derive(Clone, Copy)]
struct Scope {
    object: Shape,
    next: Shape,
    reading_key: bool,
}

impl Scope {
    const GENERIC: Self = Self {
        object: Shape::Generic,
        next: Shape::Generic,
        reading_key: false,
    };
    const ROOT: Self = Self {
        next: Shape::Project,
        ..Self::GENERIC
    };

    fn open(shape: Shape, opener: u8) -> Self {
        if opener == b'[' {
            return Self {
                next: shape.array_element(),
                ..Self::GENERIC
            };
        }
        if shape.motion_object()
            || matches!(
                shape,
                Shape::Project | Shape::Document | Shape::AnimationAssets
            )
        {
            return Self {
                object: shape,
                reading_key: true,
                ..Self::GENERIC
            };
        }
        Self::GENERIC
    }

    fn motion_slot(self) -> bool {
        (self.object.motion_object() && self.next != Shape::Generic)
            || (self.object == Shape::Generic
                && matches!(self.next, Shape::Motion | Shape::Track | Shape::Segment))
    }
}

fn typed_motion_version(rest: &[u8]) -> bool {
    // These versions decode directly into MotionClip. Keep v22's Value-based
    // migration and versions without a known schema on the generic estimate.
    matches!(rest.get(..2), Some(b"23" | b"24"))
        && rest
            .get(2)
            .is_some_and(|b| b.is_ascii_whitespace() || matches!(b, b',' | b'}'))
}

pub(super) fn document_decode_workspace(
    bytes: &[u8],
    keep_going: &mut impl FnMut() -> bool,
) -> Result<usize> {
    // Include the input and additional storage for string decoding and copies.
    // Container and member costs cover collection capacity, growth and temporary
    // serde values. Numeric text only needs the input and its value slots.
    // Scan without allocating before the syntax checker or typed decoder runs.
    let mut workspace = bytes.len();
    let mut motion_savings: usize = 0;
    // Only the path through the motion control point needs tracking. Deeper
    // containers are generic and still contribute all of their structural costs.
    let mut parents = [Scope::GENERIC; 12];
    let mut scope = Scope::ROOT;
    let mut depth: usize = 0;
    let mut typed_motions = false;
    let mut reading_version = false;
    let mut string_start: Option<usize> = None;
    let mut escaped = false;
    const CHUNK_BYTES: usize = 64 << 10;
    for (chunk_index, chunk) in bytes.chunks(CHUNK_BYTES).enumerate() {
        check(keep_going)?;
        for (offset, &byte) in chunk.iter().enumerate() {
            if let Some(start) = string_start {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    let position = chunk_index * CHUNK_BYTES + offset;
                    workspace = workspace.saturating_add(
                        (position - start + 1).saturating_mul(STRING_BYTE_ALLOWANCE),
                    );
                    string_start = None;
                    if scope.reading_key {
                        scope.next = scope.object.field(&bytes[start..position]);
                        scope.reading_key = false;
                    }
                }
                continue;
            }
            if reading_version && !byte.is_ascii_whitespace() {
                let position = chunk_index * CHUNK_BYTES + offset;
                typed_motions = typed_motion_version(&bytes[position..]);
                reading_version = false;
            }
            let overhead = match byte {
                b'"' => {
                    string_start = Some(chunk_index * CHUNK_BYTES + offset + 1);
                    continue;
                }
                b'[' | b'{' => {
                    if let Some(parent) = parents.get_mut(depth) {
                        *parent = scope;
                    }
                    depth += 1;
                    scope = Scope::open(scope.next, byte);
                    if byte == b'[' {
                        ARRAY_ALLOWANCE
                    } else {
                        if scope.object.motion_object() {
                            motion_savings = motion_savings
                                .saturating_add(OBJECT_ALLOWANCE - MOTION_OBJECT_ALLOWANCE);
                        }
                        OBJECT_ALLOWANCE
                    }
                }
                b']' | b'}' => {
                    depth = depth.saturating_sub(1);
                    scope = parents.get(depth).copied().unwrap_or(Scope::GENERIC);
                    continue;
                }
                b',' => {
                    if scope.motion_slot() {
                        motion_savings = motion_savings
                            .saturating_add(ELEMENT_ALLOWANCE - MOTION_SLOT_ALLOWANCE);
                    }
                    if scope.object != Shape::Generic {
                        scope.next = Shape::Generic;
                        scope.reading_key = true;
                    }
                    ELEMENT_ALLOWANCE
                }
                b':' => {
                    reading_version = scope.next == Shape::Version;
                    if scope.object.motion_object() && scope.next != Shape::Generic {
                        motion_savings =
                            motion_savings.saturating_add(MEMBER_ALLOWANCE - MOTION_SLOT_ALLOWANCE);
                    }
                    MEMBER_ALLOWANCE
                }
                _ => continue,
            };
            workspace = workspace.saturating_add(overhead);
        }
    }
    if let Some(start) = string_start {
        workspace =
            workspace.saturating_add((bytes.len() - start).saturating_mul(STRING_BYTE_ALLOWANCE));
    }
    // Leave documents whose version is unknown on the generic estimate.
    Ok(if typed_motions && workspace != usize::MAX {
        workspace - motion_savings
    } else {
        workspace
    })
}
