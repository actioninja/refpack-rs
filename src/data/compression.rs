////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////
pub const MAX_WINDOW_SIZE: u32 = MAX_OFFSET_DISTANCE as u32;

use std::cmp::max;
use std::collections::HashMap;
use std::io::{Read, Seek};

use crate::data::control;
use crate::data::control::{
    Command, Control, MAX_COPY_MEDIUM_OFFSET, MAX_COPY_SHORT_OFFSET, MAX_OFFSET_DISTANCE,
    MIN_COPY_LONG_LEN, MIN_COPY_MEDIUM_LEN, MIN_COPY_OFFSET,
};
use crate::{RefPackError, MAX_LITERAL_BLOCK};

//Optimization trick from libflate_lz77
//Faster lookups for very large tables
#[derive(Debug)]
enum PrefixTable {
    Small(HashMap<[u8; 3], u32>),
    Large(LargePrefixTable),
}

impl PrefixTable {
    fn new(bytes: usize) -> Self {
        if bytes < MAX_WINDOW_SIZE as usize {
            PrefixTable::Small(HashMap::new())
        } else {
            PrefixTable::Large(LargePrefixTable::new())
        }
    }

    fn insert(&mut self, prefix: [u8; 3], position: u32) -> Option<u32> {
        match *self {
            PrefixTable::Small(ref mut table) => table.insert(prefix, position),
            PrefixTable::Large(ref mut table) => table.insert(prefix, position),
        }
    }
}

#[derive(Debug)]
struct LargePrefixTable {
    table: Vec<Vec<(u8, u32)>>,
}

impl LargePrefixTable {
    fn new() -> Self {
        LargePrefixTable {
            table: (0..=0xFFFF).map(|_| Vec::new()).collect(),
        }
    }

    fn insert(&mut self, prefix: [u8; 3], position: u32) -> Option<u32> {
        let p0 = prefix[0] as usize;
        let p1 = prefix[1] as usize;
        let p2 = prefix[2];

        let index = (p0 << 8) | p1;
        let positions = &mut self.table[index];
        for &mut (key, ref mut value) in positions.iter_mut() {
            if key == p2 {
                let old = *value;
                *value = position;
                return Some(old);
            }
        }
        positions.push((p2, position));
        None
    }
}

fn prefix(input_buf: &[u8]) -> [u8; 3] {
    let buf: &[u8] = &input_buf[..3];
    [buf[0], buf[1], buf[2]]
}

/// Reads from an incoming `Read` reader and compresses and encodes to `Vec<Control>`
pub(crate) fn encode_stream(
    reader: &mut (impl Read + Seek),
    length: usize,
) -> Result<Vec<Control>, RefPackError> {
    let mut in_buffer = vec![0_u8; length];
    reader.read_exact(&mut in_buffer)?;
    let mut controls: Vec<Control> = vec![];
    let mut prefix_table = PrefixTable::new(in_buffer.len());

    let mut i = 0;
    let end = max(3, in_buffer.len()) - 3;
    let mut literal_block: Vec<u8> = Vec::with_capacity(MAX_LITERAL_BLOCK as usize);
    while i < end {
        let key = prefix(&in_buffer[i..]);

        // get the position of the prefix in the table (if it exists)
        let matched = prefix_table.insert(key, i as u32);

        let pair = matched.map(|x| x as usize).and_then(|matched| {
            let distance = i - matched;
            if distance > MAX_OFFSET_DISTANCE || distance < MIN_COPY_OFFSET as usize {
                None
            } else {
                // find the longest common prefix
                let match_length = in_buffer[i..]
                    .iter()
                    .take(control::MAX_COPY_LEN - 3)
                    .zip(&in_buffer[matched..])
                    .take_while(|(a, b)| a == b)
                    .count();

                // Insufficient similarity for given distance, reject
                if (match_length <= MIN_COPY_MEDIUM_LEN as usize
                    && distance > MAX_COPY_SHORT_OFFSET as usize)
                    || (match_length <= MIN_COPY_LONG_LEN as usize
                        && distance > MAX_COPY_MEDIUM_OFFSET as usize)
                {
                    None
                } else {
                    Some((matched, match_length))
                }
            }
        });

        if let Some((found, match_length)) = pair {
            let distance = i - found;

            // If the current literal block is longer than 3 we need to split the block
            if literal_block.len() > 3 {
                let split_point: usize = literal_block.len() - (literal_block.len() % 4);
                controls.push(Control::new_literal_block(&literal_block[..split_point]));
                let second_block = &literal_block[split_point..];
                controls.push(Control::new(
                    Command::new(distance, match_length, second_block.len()),
                    second_block.to_vec(),
                ));
            } else {
                // If it's not, just push a new block directly
                controls.push(Control::new(
                    Command::new(distance, match_length, literal_block.len()),
                    literal_block.clone(),
                ));
            }
            literal_block.clear();

            for k in (i..).take(match_length).skip(1) {
                if k >= end {
                    break;
                }
                prefix_table.insert(prefix(&in_buffer[k..]), k as u32);
            }

            i += match_length;
        } else {
            literal_block.push(in_buffer[i]);
            i += 1;
            if literal_block.len() >= (MAX_LITERAL_BLOCK as usize) {
                controls.push(Control::new_literal_block(&literal_block));
                literal_block.clear();
            }
        }
    }
    //Add remaining literals if there are any
    if i < in_buffer.len() {
        literal_block.extend_from_slice(&in_buffer[i..]);
    }
    //Extremely similar to block up above, but with a different control type
    if literal_block.len() > 3 {
        let split_point: usize = literal_block.len() - (literal_block.len() % 4);
        controls.push(Control::new_literal_block(&literal_block[..split_point]));
        controls.push(Control::new_stop(&literal_block[split_point..]));
    } else {
        controls.push(Control::new_stop(&literal_block));
    }

    Ok(controls)
}
