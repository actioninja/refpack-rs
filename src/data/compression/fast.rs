use std::cmp::{max, min};

use crate::data::compression::bytes_for_match;
use crate::data::compression::match_length::match_length;
use crate::data::compression::prefix_search::hash_table::PrefixTable;
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

struct HashChain {
    prefix_table: PrefixTable,
    hash_chain: Vec<u32>,
}

impl HashChain {
    fn new(bytes: usize) -> Self {
        Self {
            prefix_table: PrefixTable::new(bytes),
            hash_chain: vec![u32::MAX; min(bytes, LONG_OFFSET_MAX as usize)],
        }
    }

    fn insert(&mut self, prefix: [u8; 3], position: u32) -> impl Iterator<Item = u32> + use<'_> {
        let found_position = self
            .prefix_table
            .insert(prefix, position)
            .filter(|pos| position - pos <= LONG_OFFSET_MAX);
        self.hash_chain[(position % LONG_OFFSET_MAX) as usize] = found_position.unwrap_or(u32::MAX);
        found_position.into_iter().chain(HashChainIter {
            hash_chain: self,
            orig_position: position,
            cur_position: found_position,
        })
    }
}

struct HashChainIter<'a> {
    hash_chain: &'a HashChain,
    orig_position: u32,
    cur_position: Option<u32>,
}

impl Iterator for HashChainIter<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        let position = self.cur_position?;

        let next_pos = self.hash_chain.hash_chain[(position % LONG_OFFSET_MAX) as usize];
        self.cur_position =
            if next_pos == u32::MAX || self.orig_position - next_pos > LONG_OFFSET_MAX {
                None
            } else {
                Some(next_pos)
            };

        self.cur_position
    }
}

/// Reads from an incoming `Read` reader and compresses and encodes to
/// `Vec<Control>`
pub(crate) fn encode(input: &[u8]) -> Vec<Control> {
    let mut controls: Vec<Control> = vec![];
    let mut prefix_table = HashChain::new(input.len());

    let mut i = 0;
    let end = max(3, input.len()) - 3;
    let mut literal_block: Vec<u8> = Vec::with_capacity(LITERAL_MAX as usize);
    while i < end {
        let key = prefix(&input[i..]);

        // get the position of the prefix in the table (if it exists)
        let matched = prefix_table.insert(key, i as u32);

        let pair = matched
            .take(0x80)
            .filter_map(|matched| {
                let matched = matched as usize;
                let distance = i - matched;
                if distance > LONG_OFFSET_MAX as usize || distance < SHORT_OFFSET_MIN as usize {
                    None
                } else {
                    // find the longest common prefix
                    let max_copy_len = LONG_LENGTH_MAX as usize;
                    let match_length = match_length(input, i, matched, max_copy_len, 3);

                    let num_bytes = bytes_for_match(match_length, distance)?.0?;
                    Some((
                        matched,
                        match_length,
                        match_length as f64 / num_bytes as f64,
                    ))
                }
            })
            .max_by(|(_, _, r1), (_, _, r2)| r1.total_cmp(r2));

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
                let _ = prefix_table.insert(prefix(&input[k..]), k as u32);
            }

            i += match_length;
        } else {
            literal_block.push(input[i]);
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
    if i < input.len() {
        literal_block.extend_from_slice(&input[i..]);
    }
    // Extremely similar to block up above, but with a different control type
    if literal_block.len() > 3 {
        let split_point: usize = literal_block.len() - (literal_block.len() % 4);
        controls.push(Control::new_literal_block(&literal_block[..split_point]));
        controls.push(Control::new_stop(&literal_block[split_point..]));
    } else {
        controls.push(Control::new_stop(&literal_block));
    }

    controls
}
