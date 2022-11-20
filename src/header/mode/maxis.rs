////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::io::{Read, Seek, Write};

use crate::header::mode::Mode;
use crate::header::Header;
use crate::RefPackResult;

pub struct Maxis;

impl Mode for Maxis {
    const LENGTH: usize = 9;

    fn read<R: Read + Seek>(reader: &mut R) -> RefPackResult<Header> {
        todo!()
    }

    fn write<W: Write + Seek>(header: Header, writer: &mut W) -> RefPackResult<()> {
        todo!()
    }
}
