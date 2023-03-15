////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! Possible compression formats to utilize
//!
//!
use crate::data::control::mode::{Reference as ReferenceControl, Simcity4 as Simcity4Control};
use crate::data::control::Mode as ControlMode;
use crate::header::mode::{Maxis, Maxis2, Mode as HeaderMode, Reference as ReferenceHeader};

/// Trait that represents a pair of Header Modes and Control Modes that define a compression format.
///
/// This trait is referenced entirely via associated functions at compile time and gets entirely
/// monomorphized out. It solely exists to reference associated functions on the two control types.
/// It should not be implemented on structs that are intended to be constructed, and it's
/// recommended to add a `()` typed private field to prevent them from being constructed.
pub trait Format {
    /// The header read/write mode to be used for compression and decompression.
    ///
    /// [HeaderMode](crate::header::mode::Mode) is an alias of the mode in the header module.
    type HeaderMode: HeaderMode;
    /// The body control code read/write mode to be used for compression and decompression.
    ///
    /// [ControlMode](crate::data::control::mode) is an alias of the mode in the control module.
    type ControlMode: ControlMode;
}

/// Reference implementation as originally made in the 90s.
/// - Shortened header compared to later implementations, only encodes decompressed size ([Reference](crate::header::mode::Reference))
/// - Standard control codes ([Reference](crate::data::control::mode::Reference))
pub struct Reference {
    // trick to prevent struct from ever being constructed. These are "markers" intended to be used
    // as generic arguments rather than data structs
    _private: (),
}

impl Format for Reference {
    type HeaderMode = ReferenceHeader;
    type ControlMode = ReferenceControl;
}

/// Format utilized by The Sims games from Sims 1 to 2
/// - Uses standard [Maxis](crate::header::mode::Maxis) header
/// - Standard control codes ([Reference](crate::data::control::mode::Reference))
pub struct TheSims12 {
    // trick to prevent struct from ever being constructed. These are "markers" intended to be used
    // as generic arguments rather than data structs
    _private: (),
}

impl Format for TheSims12 {
    type HeaderMode = Maxis;
    type ControlMode = ReferenceControl;
}

/// Format utilized by Simcity 4.
/// - Uses standard [Maxis](crate::header::mode::Maxis) header
/// - Nonstandard long control code. See [Simcity4](crate::data::control::mode::Simcity4)
pub struct Simcity4 {
    // trick to prevent struct from ever being constructed. These are "markers" intended to be used
    // as generic arguments rather than data structs
    _private: (),
}

impl Format for Simcity4 {
    type HeaderMode = Maxis;
    type ControlMode = Simcity4Control;
}

/// Format utilized by Simcity 4.
/// - Uses new [Maxis2](crate::header::mode::Maxis2) header
/// - Standard control codes ([Reference](crate::data::control::mode::Reference))
pub struct TheSims34 {
    // trick to prevent struct from ever being constructed. These are "markers" intended to be used
    // as generic arguments rather than data structs
    _private: (),
}

impl Format for TheSims34 {
    type HeaderMode = Maxis2;
    type ControlMode = ReferenceControl;
}
