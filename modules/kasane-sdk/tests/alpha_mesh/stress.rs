//! These complement the daily samples: full Cargo runs cover every original seed case once.
use super::mask_cases::*;

#[test]
fn remaining_padded_fragmented_masks() {
    for (case, alpha) in fragmented_masks().filter(|(case, _)| !FRAGMENTED_DAILY.contains(case)) {
        check_fragmented(case, &alpha);
    }
}

#[test]
fn remaining_small_masks() {
    for (case, alpha) in small_masks().filter(|(case, _)| !SMALL_DAILY.contains(case)) {
        check_small(case, &alpha);
    }
}
