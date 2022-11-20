////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use crate::data::control::mode::Reference as ReferenceControl;
use crate::data::control::Mode as ControlMode;
use crate::header::mode::{Maxis, Mode as HeaderMode, Reference as ReferenceHeader};

pub trait Format {
    type HeaderMode: HeaderMode;
    type ControlMode: ControlMode;
}

pub struct Reference;

impl Format for Reference {
    type HeaderMode = ReferenceHeader;
    type ControlMode = ReferenceControl;
}

pub struct TheSims;

impl Format for TheSims {
    type HeaderMode = Maxis;
    type ControlMode = ReferenceControl;
}
