//! The ChaCha20 block function. Defined in RFC 8439 Section 2.3.
//!
//! <https://tools.ietf.org/html/rfc8439#section-2.3>
//!
//! SSE2-optimized implementation for x86/x86-64 CPUs.

use crate::{CONSTANTS, IV_SIZE, KEY_SIZE};
use core::convert::TryInto;

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[derive(Clone)]
pub(crate) struct Block {
    v0: __m128i,
    v1: __m128i,
    v2: __m128i,
    v3: __m128i,
    rounds: usize,
}

impl Block {
    /// Initialize block function with the given key size, IV, and number of rounds
    pub(crate) fn new(key: &[u8; KEY_SIZE], iv: [u8; IV_SIZE], rounds: usize) -> Self {
        assert!(
            rounds == 8 || rounds == 12 || rounds == 20,
            "rounds must be 8, 12, or 20"
        );

        let vs = unsafe { init(key, iv, 0) };

        Self {
            v0: vs[0],
            v1: vs[1],
            v2: vs[2],
            v3: vs[3],
            rounds,
        }
    }

    #[inline]
    pub(crate) fn generate(&self, counter: u64, output: &mut [u8]) {
        let (mut v0, mut v1, mut v2) = (self.v0, self.v1, self.v2);

        let v3_init = unsafe { set_counter(self.v3, counter) };
        let mut v3 = v3_init;

        for _ in 0..(self.rounds / 2) {
            unsafe { double_quarter_round(&mut v0, &mut v1, &mut v2, &mut v3) };
        }

        unsafe {
            self.add_and_store(v0, v1, v2, v3, v3_init, output);
        }
    }

    #[inline]
    #[target_feature(enable = "sse2")]
    unsafe fn add_and_store(
        &self,
        v0: __m128i,
        v1: __m128i,
        v2: __m128i,
        v3: __m128i,
        v3_init: __m128i,
        output: &mut [u8],
    ) {
        store(
            _mm_add_epi32(v0, self.v0),
            _mm_add_epi32(v1, self.v1),
            _mm_add_epi32(v2, self.v2),
            _mm_add_epi32(v3, v3_init),
            output,
        )
    }
}

#[inline]
#[target_feature(enable = "sse2")]
#[allow(clippy::cast_ptr_alignment)] // loadu supports unaligned loads
unsafe fn init(key: &[u8; KEY_SIZE], iv: [u8; IV_SIZE], counter: u64) -> [__m128i; 4] {
    let v0 = _mm_loadu_si128(CONSTANTS.as_ptr() as *const __m128i);
    let v1 = _mm_loadu_si128(key.as_ptr().offset(0x00) as *const __m128i);
    let v2 = _mm_loadu_si128(key.as_ptr().offset(0x10) as *const __m128i);
    let v3 = _mm_set_epi32(
        i32::from_le_bytes(iv[4..].try_into().unwrap()),
        i32::from_le_bytes(iv[..4].try_into().unwrap()),
        ((counter >> 32) & 0xffff_ffff) as i32,
        (counter & 0xffff_ffff) as i32,
    );

    [v0, v1, v2, v3]
}

#[inline]
#[target_feature(enable = "sse2")]
unsafe fn set_counter(v3: __m128i, counter: u64) -> __m128i {
    _mm_or_si128(
        v3,
        _mm_set_epi32(
            0,
            0,
            ((counter >> 32) & 0xffff_ffff) as i32,
            (counter & 0xffff_ffff) as i32,
        ),
    )
}

#[inline]
#[target_feature(enable = "sse2")]
#[allow(clippy::cast_ptr_alignment)] // storeu supports unaligned stores
unsafe fn store(v0: __m128i, v1: __m128i, v2: __m128i, v3: __m128i, output: &mut [u8]) {
    _mm_storeu_si128(output.as_mut_ptr().offset(0x00) as *mut __m128i, v0);
    _mm_storeu_si128(output.as_mut_ptr().offset(0x10) as *mut __m128i, v1);
    _mm_storeu_si128(output.as_mut_ptr().offset(0x20) as *mut __m128i, v2);
    _mm_storeu_si128(output.as_mut_ptr().offset(0x30) as *mut __m128i, v3);
}

#[inline]
#[target_feature(enable = "sse2")]
unsafe fn double_quarter_round(
    v0: &mut __m128i,
    v1: &mut __m128i,
    v2: &mut __m128i,
    v3: &mut __m128i,
) {
    add_xor_rot(v0, v1, v2, v3);
    rows_to_cols(v0, v1, v2, v3);
    add_xor_rot(v0, v1, v2, v3);
    cols_to_rows(v0, v1, v2, v3);
}

#[inline]
#[target_feature(enable = "sse2")]
unsafe fn rows_to_cols(_v0: &mut __m128i, v1: &mut __m128i, v2: &mut __m128i, v3: &mut __m128i) {
    // v1 >>>= 32; v2 >>>= 64; v3 >>>= 96;
    *v1 = _mm_shuffle_epi32(*v1, 0b_00_11_10_01); // _MM_SHUFFLE(0, 3, 2, 1)
    *v2 = _mm_shuffle_epi32(*v2, 0b_01_00_11_10); // _MM_SHUFFLE(1, 0, 3, 2)
    *v3 = _mm_shuffle_epi32(*v3, 0b_10_01_00_11); // _MM_SHUFFLE(2, 1, 0, 3)
}

#[inline]
#[target_feature(enable = "sse2")]
unsafe fn cols_to_rows(_v0: &mut __m128i, v1: &mut __m128i, v2: &mut __m128i, v3: &mut __m128i) {
    // v1 <<<= 32; v2 <<<= 64; v3 <<<= 96;
    *v1 = _mm_shuffle_epi32(*v1, 0b_10_01_00_11); // _MM_SHUFFLE(2, 1, 0, 3)
    *v2 = _mm_shuffle_epi32(*v2, 0b_01_00_11_10); // _MM_SHUFFLE(1, 0, 3, 2)
    *v3 = _mm_shuffle_epi32(*v3, 0b_00_11_10_01); // _MM_SHUFFLE(0, 3, 2, 1)
}

#[inline]
#[target_feature(enable = "sse2")]
unsafe fn add_xor_rot(v0: &mut __m128i, v1: &mut __m128i, v2: &mut __m128i, v3: &mut __m128i) {
    // v0 += v1; v3 ^= v0; v3 <<<= (16, 16, 16, 16);
    *v0 = _mm_add_epi32(*v0, *v1);
    *v3 = _mm_xor_si128(*v3, *v0);
    *v3 = _mm_xor_si128(_mm_slli_epi32(*v3, 16), _mm_srli_epi32(*v3, 16));

    // v2 += v3; v1 ^= v2; v1 <<<= (12, 12, 12, 12);
    *v2 = _mm_add_epi32(*v2, *v3);
    *v1 = _mm_xor_si128(*v1, *v2);
    *v1 = _mm_xor_si128(_mm_slli_epi32(*v1, 12), _mm_srli_epi32(*v1, 20));

    // v0 += v1; v3 ^= v0; v3 <<<= (8, 8, 8, 8);
    *v0 = _mm_add_epi32(*v0, *v1);
    *v3 = _mm_xor_si128(*v3, *v0);
    *v3 = _mm_xor_si128(_mm_slli_epi32(*v3, 8), _mm_srli_epi32(*v3, 24));

    // v2 += v3; v1 ^= v2; v1 <<<= (7, 7, 7, 7);
    *v2 = _mm_add_epi32(*v2, *v3);
    *v1 = _mm_xor_si128(*v1, *v2);
    *v1 = _mm_xor_si128(_mm_slli_epi32(*v1, 7), _mm_srli_epi32(*v1, 25));
}

#[cfg(all(test, target_feature = "sse2"))]
mod tests {
    use super::*;
    use crate::{block::soft::Block as SoftBlock, BLOCK_SIZE};
    use core::convert::TryInto;

    // random inputs for testing
    const R_CNT: u64 = 0x9fe625b6d23a8fa8u64;
    const R_IV: [u8; IV_SIZE] = [0x2f, 0x96, 0xa8, 0x4a, 0xf8, 0x92, 0xbc, 0x94];
    const R_KEY: [u8; KEY_SIZE] = [
        0x11, 0xf2, 0x72, 0x99, 0xe1, 0x79, 0x6d, 0xef, 0xb, 0xdc, 0x6a, 0x58, 0x1f, 0x1, 0x58,
        0x94, 0x92, 0x19, 0x69, 0x3f, 0xe9, 0x35, 0x16, 0x72, 0x63, 0xd1, 0xd, 0x94, 0x6d, 0x31,
        0x34, 0x11,
    ];

    #[test]
    fn init_and_store() {
        unsafe {
            let vs = init(&R_KEY, R_IV, R_CNT);

            let mut output = [0u8; BLOCK_SIZE];
            store(vs[0], vs[1], vs[2], vs[3], &mut output);

            let expected = [
                1634760805, 857760878, 2036477234, 1797285236, 2574447121, 4016929249, 1483398155,
                2488795423, 1063852434, 1914058217, 2483933539, 288633197, 3527053224, 2682660278,
                1252562479, 2495386360,
            ];

            for (i, chunk) in output.chunks(4).enumerate() {
                assert_eq!(expected[i], u32::from_le_bytes(chunk.try_into().unwrap()));
            }
        }
    }

    #[test]
    fn init_and_double_round() {
        unsafe {
            let vs = init(&R_KEY, R_IV, R_CNT);
            let mut v0 = vs[0];
            let mut v1 = vs[1];
            let mut v2 = vs[2];
            let mut v3 = vs[3];

            double_quarter_round(&mut v0, &mut v1, &mut v2, &mut v3);

            let mut output = [0u8; BLOCK_SIZE];
            store(v0, v1, v2, v3, &mut output);

            let expected = [
                562456049, 3130322832, 1534507163, 1938142593, 1427879055, 3727017100, 1549525649,
                2358041203, 1010155040, 657444539, 2865892668, 2826477124, 737507996, 3254278724,
                3376929372, 928763221,
            ];

            for (i, chunk) in output.chunks(4).enumerate() {
                assert_eq!(expected[i], u32::from_le_bytes(chunk.try_into().unwrap()));
            }
        }
    }

    #[test]
    fn generate_vs_scalar_impl() {
        let mut soft_result = [0u8; BLOCK_SIZE];
        SoftBlock::new(&R_KEY, R_IV, 20).generate(R_CNT, &mut soft_result);

        let mut simd_result = [0u8; BLOCK_SIZE];
        Block::new(&R_KEY, R_IV, 20).generate(R_CNT, &mut simd_result);

        assert_eq!(&soft_result[..], &simd_result[..])
    }
}