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
use crate::header::mode::{Maxis, Mode as HeaderMode, Reference as ReferenceHeader};

/// Marker trait. Implement on a unit to make them usable as arguments to compression functions
pub trait Format {
    type HeaderMode: HeaderMode;
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
pub struct TheSims {
    // trick to prevent struct from ever being constructed. These are "markers" intended to be used
    // as generic arguments rather than data structs
    _private: (),
}

impl Format for TheSims {
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
