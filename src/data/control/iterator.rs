////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::io::{Read, Seek};

use crate::data::control::Control;

/// Iterator to to read a byte reader into a sequence of controls
pub struct Iter<'a, R: Read + Seek> {
    reader: &'a mut R,
    reached_stop: bool,
}

impl<'a, R: Read + Seek> Iter<'a, R> {
    pub fn new(reader: &'a mut R) -> Iter<'a, R> {
        Iter::<'a, R> {
            reader,
            reached_stop: false,
        }
    }
}

impl<'a, R: Read + Seek> Iterator for Iter<'a, R> {
    type Item = Control;

    fn next(&mut self) -> Option<Self::Item> {
        if self.reached_stop {
            None
        } else {
            Control::read(self.reader).ok().map(|control| {
                if control.command.is_stop() {
                    self.reached_stop = true;
                }
                control
            })
        }
    }
}

#[cfg(test)]
mod test {
    use std::io::Cursor;

    use proptest::prop_assert_eq;
    use test_strategy::proptest;

    use super::*;
    use crate::data::control::tests::generate_valid_control_sequence;
    use crate::data::control::Control;

    #[proptest]
    fn test_control_iterator(
        #[strategy(generate_valid_control_sequence(500))] input: Vec<Control>,
    ) {
        let expected = input.clone();
        let buf = input
            .iter()
            .map(|control: &Control| -> Vec<u8> {
                let mut buf = Cursor::new(vec![]);
                control.write(&mut buf).unwrap();
                buf.into_inner()
            })
            .fold(vec![], |mut acc, mut buf| {
                acc.append(&mut buf);
                acc
            });

        let mut cursor = Cursor::new(buf);
        let out: Vec<Control> = Iter::new(&mut cursor).collect();

        prop_assert_eq!(out, expected);
    }
}
