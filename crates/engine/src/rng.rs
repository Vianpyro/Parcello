//! Deterministic PRNG (`SplitMix64`). Kept dependency-free so the engine's
//! randomness is fully reproducible from the seed stored in `GameState`.

pub const fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Unbiased-enough for game purposes; modulo bias over u64 is negligible
/// for bounds this small.
pub fn below(state: &mut u64, bound: u64) -> u64 {
    debug_assert!(bound > 0);
    next(state) % bound
}

/// Fisher-Yates in-place shuffle.
pub fn shuffle<T>(items: &mut [T], state: &mut u64) {
    for i in (1..items.len()).rev() {
        let j = below(state, (i + 1) as u64) as usize;
        items.swap(i, j);
    }
}
