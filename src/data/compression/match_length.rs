////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::cmp::min;

use crate::data::control::LONG_LENGTH_MAX;

const USIZE_BYTES: usize = size_of::<usize>();

#[inline(always)]
fn compare_block(src: [u8; USIZE_BYTES], cmp: [u8; USIZE_BYTES]) -> Option<usize> {
    let src_int = usize::from_ne_bytes(src);
    let cmp_int = usize::from_ne_bytes(cmp);

    let xor = src_int ^ cmp_int;

    if xor == 0 {
        None
    } else {
        Some((xor.to_le().trailing_zeros() / 8) as usize)
    }
}

#[inline(always)]
fn match_length_blocks(src: &[u8], cmp: &[u8]) -> Option<usize> {
    // when using fixed length slices this gets optimized to a (fused) simd load and compare
    if src == cmp {
        return None;
    }

    let src_chunks = src.chunks_exact(USIZE_BYTES);
    let cmp_chunks = cmp.chunks_exact(USIZE_BYTES);

    src_chunks
        .zip(cmp_chunks)
        .enumerate()
        .find_map(|(i, (src, cmp))| {
            compare_block(src.try_into().unwrap(), cmp.try_into().unwrap())
                .map(|found| i * USIZE_BYTES + found)
        })
}

#[inline]
fn match_length_simd(buffer: &[u8], source: usize, matched_pos: usize, max_len: usize) -> usize {
    const LANES: usize = 16;

    if source + USIZE_BYTES < buffer.len() {
        if let Some(found) = compare_block(
            buffer[source..source + USIZE_BYTES].try_into().unwrap(),
            buffer[matched_pos..matched_pos + USIZE_BYTES]
                .try_into()
                .unwrap(),
        ) {
            return min(found, max_len);
        }
        if max_len <= USIZE_BYTES {
            return max_len;
        }

        let source_slice = &buffer[source + USIZE_BYTES..min(source + max_len, buffer.len())];
        let match_slice = &buffer[matched_pos + USIZE_BYTES..];

        let source_chunks = source_slice.chunks_exact(LANES);
        let match_chunks = match_slice.chunks_exact(LANES);
        let source_chunks_remainder = source_chunks.remainder();

        let mut num = USIZE_BYTES;
        for (src, cmp) in source_chunks.zip(match_chunks) {
            if let Some(found) = match_length_blocks(src, cmp) {
                return num + found;
            }
            num += LANES;
        }

        source_chunks_remainder
            .iter()
            .zip(match_slice[num - USIZE_BYTES..].iter())
            .take_while(|(a, b)| a == b)
            .count()
            + num
    } else {
        let source_slice = &buffer[source..min(source + max_len, buffer.len())];
        let match_slice = &buffer[matched_pos..];

        source_slice
            .iter()
            .zip(match_slice.iter())
            .take_while(|(a, b)| a == b)
            .count()
    }
}

/// find the length of common bytes between two positions in a buffer
#[inline]
pub fn match_length(
    buffer: &[u8],
    source: usize,
    matched_pos: usize,
    max_len: usize,
    skip: usize,
) -> usize {
    debug_assert!(matched_pos < source);

    match_length_simd(buffer, source + skip, matched_pos + skip, max_len - skip) + skip
}

/// does the byte at the two positions with the specified offset (`skip`) match?
pub fn byte_offset_matches(buffer: &[u8], source: usize, matched_pos: usize, skip: usize) -> bool {
    debug_assert!(matched_pos < source);

    let source_idx = source + skip;

    if source_idx >= buffer.len() {
        return false;
    }

    let match_idx = matched_pos + skip;

    buffer[source_idx] == buffer[match_idx]
}

/// check for any sequence of bytes that matches exactly `except_match_length` bytes with the source position
///
/// For any position that fulfils this condition the function will return `except_match_length` + 1.
/// Any other position will return the match length of that position up to `except_match_length` bytes.
///
/// `skip` can be used to specify the number of bytes that is already known to be matching
pub fn match_length_except(
    buffer: &[u8],
    source: usize,
    bad_match_pos: usize,
    matched_pos: usize,
    except_match_length: usize,
    skip: usize,
) -> u16 {
    let match_len = match_length(buffer, source, matched_pos, except_match_length + 1, skip);
    (if match_len == except_match_length {
        except_match_length
            + usize::from(!byte_offset_matches(
                buffer,
                bad_match_pos,
                matched_pos,
                except_match_length,
            ))
    } else {
        min(match_len, except_match_length)
    }) as u16
}

/// check for any sequence of bytes that matches with `matched_pos`
/// OR matches `or_match_pos` exactly `or_match_length` bytes
///
/// `skip` can be used to specify the number of bytes that is already known to be matching
pub fn match_length_or(
    buffer: &[u8],
    source: usize,
    matched_pos: usize,
    or_match_pos: usize,
    or_match_length: usize,
    skip: usize,
) -> u16 {
    let match_len = match_length(buffer, source, matched_pos, LONG_LENGTH_MAX as usize, skip);
    (if match_len == or_match_length {
        match_len
            + usize::from(!byte_offset_matches(
                buffer,
                or_match_pos,
                matched_pos,
                match_len,
            ))
    } else {
        match_len
    }) as u16
}
