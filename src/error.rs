////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("No input provided to compression")]
    EmptyInput,
    #[error("Invalid magic number at compression header `{0:#04X}`")]
    InvalidMagic(u16),
    #[error("IO Error")]
    Io(#[from] std::io::Error),
}
