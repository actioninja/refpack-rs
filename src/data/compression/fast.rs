use std::cmp::max;
use std::collections::HashMap;
use std::io::{Read, Seek};

use crate::data::compression::bytes_for_match;
use crate::data::compression::match_length::match_length;
use crate::data::compression::prefix_search::prefix;
use crate::data::control::{
    Command,
    Control,
    COPY_LITERAL_MAX,
    LITERAL_MAX,
    LONG_LENGTH_MAX,
    LONG_OFFSET_MAX,
    SHORT_OFFSET_MIN,
};
use crate::RefPackError;

// Optimization trick from libflate_lz77
// Faster lookups for very large tables
#[derive(Debug)]
pub enum PrefixTable {
    Small(HashMap<[u8; 3], Vec<u32>>),
    Large(LargePrefixTable),
}

impl PrefixTable {
    pub fn new(bytes: usize) -> Self {
        if bytes < LONG_OFFSET_MAX as usize {
            PrefixTable::Small(HashMap::new())
        } else {
            PrefixTable::Large(LargePrefixTable::new())
        }
    }

    pub fn insert(&mut self, prefix: [u8; 3], position: u32) -> Option<Vec<u32>> {
        match *self {
            PrefixTable::Small(ref mut table) => {
                if let Some(vec) = table.get_mut(&prefix) {
                    let out = vec.iter().rev().take(0x80).copied().collect();
                    vec.push(position);
                    Some(out)
                } else {
                    table.insert(prefix, vec![position]);
                    None
                }
            }
            PrefixTable::Large(ref mut table) => table.insert(prefix, position),
        }
    }
}

#[derive(Debug)]
pub struct LargePrefixTable {
    table: Vec<Vec<(u8, Vec<u32>)>>,
}

impl LargePrefixTable {
    fn new() -> Self {
        LargePrefixTable {
            table: (0..=0xFFFF).map(|_| Vec::new()).collect(),
        }
    }

    fn insert(&mut self, prefix: [u8; 3], position: u32) -> Option<Vec<u32>> {
        let p0 = prefix[0] as usize;
        let p1 = prefix[1] as usize;
        let p2 = prefix[2];

        let index = (p0 << 8) | p1;
        let positions = &mut self.table[index];
        for &mut (key, ref mut value) in &mut *positions {
            if key == p2 {
                let old = value.iter().rev().take(0x80).copied().collect();
                value.push(position);
                return Some(old);
            }
        }
        positions.push((p2, vec![position]));
        None
    }
}

/// Reads from an incoming `Read` reader and compresses and encodes to
/// `Vec<Control>`
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
    let mut literal_block: Vec<u8> = Vec::with_capacity(LITERAL_MAX as usize);
    while i < end {
        let key = prefix(&in_buffer[i..]);

        // get the position of the prefix in the table (if it exists)
        let matched = prefix_table.insert(key, i as u32);

        let pair = matched.and_then(|x| {
            x.into_iter()
                .filter_map(|matched| {
                    let matched = matched as usize;
                    let distance = i - matched;
                    if distance > LONG_OFFSET_MAX as usize || distance < SHORT_OFFSET_MIN as usize {
                        None
                    } else {
                        // find the longest common prefix
                        let max_copy_len = LONG_LENGTH_MAX as usize;
                        let match_length =
                            match_length(&in_buffer, i, matched, max_copy_len - 3, 0);

                        let num_bytes = bytes_for_match(match_length, distance)?.0?;
                        Some((
                            matched,
                            match_length,
                            match_length as f64 / num_bytes as f64,
                        ))
                    }
                })
                .max_by(|(_, _, r1), (_, _, r2)| r1.total_cmp(r2))
        });

        if let Some((found, match_length, _)) = pair {
            let distance = i - found;

            // If the current literal block is longer than the copy limit we need to split the block
            if literal_block.len() > COPY_LITERAL_MAX as usize {
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
            // If it's reached the limit, push the block immediately and clear the running
            // block
            if literal_block.len() >= (LITERAL_MAX as usize) {
                controls.push(Control::new_literal_block(&literal_block));
                literal_block.clear();
            }
        }
    }
    // Add remaining literals if there are any
    if i < in_buffer.len() {
        literal_block.extend_from_slice(&in_buffer[i..]);
    }
    // Extremely similar to block up above, but with a different control type
    if literal_block.len() > 3 {
        let split_point: usize = literal_block.len() - (literal_block.len() % 4);
        controls.push(Control::new_literal_block(&literal_block[..split_point]));
        controls.push(Control::new_stop(&literal_block[split_point..]));
    } else {
        controls.push(Control::new_stop(&literal_block));
    }

    Ok(controls)
}
