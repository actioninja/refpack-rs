////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::io::{Read, Seek};
use std::marker::PhantomData;

use crate::data::control::mode::Mode;
use crate::data::control::Control;

/// Iterator to to read a byte reader into a sequence of controls that can be iterated through
pub struct Iter<'a, R: Read + Seek, M: Mode> {
    reader: &'a mut R,
    reached_stop: bool,
    mode: PhantomData<M>,
}

impl<'a, R: Read + Seek, M: Mode> Iter<'a, R, M> {
    pub fn new(reader: &'a mut R) -> Iter<'a, R, M> {
        Iter::<'a, R, M> {
            reader,
            reached_stop: false,
            mode: PhantomData,
        }
    }
}

impl<'a, R: Read + Seek, M: Mode> Iterator for Iter<'a, R, M> {
    type Item = Control;

    fn next(&mut self) -> Option<Self::Item> {
        if self.reached_stop {
            None
        } else {
            Control::read::<M>(self.reader).ok().map(|control| {
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
    use crate::data::control::mode::Reference;
    use crate::data::control::{Command, Control};

    #[proptest]
    fn test_control_iterator(input: Vec<Control>) {
        //todo: make this not a stupid hack
        let mut input: Vec<Control> = input
            .iter()
            .filter(|c| !c.command.is_stop())
            .cloned()
            .collect();
        input.push(Control {
            command: Command::new_stop::<Reference>(0),
            bytes: vec![],
        });
        let expected = input.clone();
        let buf = input
            .iter()
            .map(|control: &Control| -> Vec<u8> {
                let mut buf = Cursor::new(vec![]);
                control.write::<Reference>(&mut buf).unwrap();
                buf.into_inner()
            })
            .fold(vec![], |mut acc, mut buf| {
                acc.append(&mut buf);
                acc
            });

        let mut cursor = Cursor::new(buf);
        let out: Vec<Control> = Iter::<_, Reference>::new(&mut cursor).collect();

        prop_assert_eq!(out, expected);
    }
}
