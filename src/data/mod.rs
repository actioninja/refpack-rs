////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////
pub mod compression;
pub mod control;
pub mod decompression;

pub(crate) fn copy_within_slice(v: &mut [impl Copy], from: usize, to: usize, len: usize) {
    if from > to {
        let (dst, src) = v.split_at_mut(from);
        dst[to..to + len].copy_from_slice(&src[..len]);
    } else {
        let (src, dst) = v.split_at_mut(to);
        dst[..len].copy_from_slice(&src[from..from + len]);
    }
}
