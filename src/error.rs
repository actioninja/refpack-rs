////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use binrw::Error as BinRWError;

pub enum Error {
    EmptyInput,
    InvalidMagic(u16),
    Io(std::io::Error),
    BinRW(BinRWError),
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<BinRWError> for Error {
    fn from(err: BinRWError) -> Self {
        match err {
            BinRWError::Io(io) => Error::Io(io),
            //These cases should never happen just from the usage of BinRW, but they're wrapped just in case
            BinRWError::Backtrace(_)
            | BinRWError::AssertFail { .. }
            | BinRWError::Custom { .. }
            | BinRWError::EnumErrors { .. }
            | BinRWError::NoVariantMatch { .. }
            | BinRWError::BadMagic { .. } => Error::BinRW(err),
            _ => unreachable!(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "Empty input provided to compression"),
            Self::InvalidMagic(magic) => write!(f, "Invalid magic: 0x{:04X}", magic),
            Self::Io(err) => std::fmt::Display::fmt(err, f),
            Self::BinRW(err) => std::fmt::Display::fmt(err, f),
        }
    }
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        <Error as std::fmt::Display>::fmt(self, f)
    }
}
