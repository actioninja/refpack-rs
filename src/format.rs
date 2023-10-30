////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! Possible compression formats to utilize
use crate::header::mode::{
    Maxis as MaxisHeader,
    Mode as HeaderMode,
    Reference as ReferenceHeader,
    SimEA as SimEAHeader,
};

/// Trait that represents a format to be utilized for compression
///
/// This trait is referenced entirely via associated functions at compile time
/// and gets entirely monomorphized out. It solely exists to reference
/// associated functions on the control types. It should not be implemented
/// on structs that are intended to be constructed, and should generally only be
/// implemented on unconstructable ZST structs or empty enums.
pub trait Format {
    /// The header read/write mode to be used for compression and decompression.
    ///
    /// [HeaderMode](crate::header::mode::Mode) is an alias of the mode in the
    /// header module.
    type HeaderMode: HeaderMode;
}

/// Reference implementation as originally made in the 90s.
/// - Shortened header compared to later implementations, only encodes
///   decompressed size ([Reference](crate::header::mode::Reference))
pub enum Reference {}

impl Format for Reference {
    type HeaderMode = ReferenceHeader;
}

/// Format utilized by The Sims games from Sims 1 to 2, as well as Simcity 4
/// - Uses standard [Maxis](crate::header::mode::Maxis) header
pub enum Maxis {}

impl Format for Maxis {
    type HeaderMode = MaxisHeader;
}

/// Format utilized by The Sims 3 and Sims 4.
/// - Uses new [SimEA](crate::header::mode::SimEA) header
pub enum SimEA {}

impl Format for SimEA {
    type HeaderMode = SimEAHeader;
}
