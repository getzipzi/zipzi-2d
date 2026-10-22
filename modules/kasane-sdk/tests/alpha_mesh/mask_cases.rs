//! Fixed-seed matrices shared by daily representatives and the remaining stress cases.
use super::common::*;
use kasane_sdk::AlphaMeshOptions;

pub const FRAGMENTED_DAILY: [usize; 4] = [0, 7, 15, 23];
pub const SMALL_DAILY: [usize; 4] = [0, 15, 31, 63];

fn masks(
    mut state: u32,
    count: usize,
    pixels: usize,
    opaque: fn(u32) -> bool,
) -> impl Iterator<Item = (usize, Vec<u8>)> {
    (0..count).map(move |case| {
        let alpha = (0..pixels)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                u8::from(opaque(state)) * 255
            })
            .collect();
        (case, alpha)
    })
}

pub fn fragmented_masks() -> impl Iterator<Item = (usize, Vec<u8>)> {
    masks(0x532e_a90d, 24, 24 * 24, |state| state.is_multiple_of(5))
}

pub fn small_masks() -> impl Iterator<Item = (usize, Vec<u8>)> {
    masks(0x12ab_34cd, 64, 64, |state| state & 1 != 0)
}

pub fn check_fragmented(case: usize, alpha: &[u8]) {
    eprintln!("fragmented mask case {case}");
    let opts = AlphaMeshOptions {
        outside_margin: 1.5,
        minimum_margin: 0.5,
        inside_margin: 0.5,
        ..options()
    };
    let mesh = generate(24, 24, alpha, &opts);
    valid(&mesh, 24, 24);
    coverage(&mesh, 24, 24, alpha, 0, false);
}

pub fn check_small(case: usize, alpha: &[u8]) {
    eprintln!("small mask case {case}");
    let mesh = generate(8, 8, alpha, &options());
    valid(&mesh, 8, 8);
    coverage(&mesh, 8, 8, alpha, 0, true);
    assert!((area(&mesh) - alpha.iter().filter(|&&v| v > 0).count() as f64).abs() < 1e-4);
}

#[test]
fn fixed_seed_matrices_keep_the_original_inputs() {
    // Checksums captured from the original unsplit random loops, before
    // selecting daily cases. Guard against resetting the PRNG when skipping.
    for (cases, daily, count, expected) in [
        (
            fragmented_masks().collect::<Vec<_>>(),
            FRAGMENTED_DAILY,
            24,
            0xcb13_2dd5_142b_d10f_u64,
        ),
        (
            small_masks().collect::<Vec<_>>(),
            SMALL_DAILY,
            64,
            0x181a_5317_da03_38be_u64,
        ),
    ] {
        assert_eq!(cases.len(), count);
        assert!(daily.iter().all(|&case| case < count));
        assert!(daily.windows(2).all(|pair| pair[0] < pair[1]));
        let checksum = cases
            .iter()
            .flat_map(|(_, alpha)| alpha)
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, &value| {
                (hash ^ u64::from(value)).wrapping_mul(0x100_0000_01b3)
            });
        assert_eq!(checksum, expected);
    }
}
