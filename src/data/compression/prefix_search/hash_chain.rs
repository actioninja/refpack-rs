use std::cmp::min;

use crate::data::compression::match_length::match_length;
use crate::data::compression::prefix_search::hash_table::PrefixTable;
use crate::data::compression::prefix_search::{prefix, PrefixSearcher, HASH_CHAIN_MODULO};
use crate::data::control::{LONG_LENGTH_MAX, LONG_OFFSET_MAX};

pub(crate) struct HashChain {
    prefix_table: PrefixTable,
    hash_chain: Vec<u32>,
}

impl HashChain {
    pub fn new(bytes: usize) -> Self {
        Self {
            prefix_table: PrefixTable::new(bytes),
            hash_chain: vec![u32::MAX; min(bytes, HASH_CHAIN_MODULO)],
        }
    }

    pub fn insert(
        &mut self,
        prefix: [u8; 3],
        position: u32,
    ) -> impl Iterator<Item = u32> + use<'_> {
        let found_position = self
            .prefix_table
            .insert(prefix, position)
            .filter(|pos| position - pos <= LONG_OFFSET_MAX);
        self.hash_chain[position as usize % HASH_CHAIN_MODULO] = found_position.unwrap_or(u32::MAX);
        found_position.into_iter().chain(HashChainIter {
            hash_chain: self,
            orig_position: position,
            cur_position: found_position,
        })
    }
}

pub(crate) struct HashChainIter<'a> {
    hash_chain: &'a HashChain,
    orig_position: u32,
    cur_position: Option<u32>,
}

impl Iterator for HashChainIter<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        let position = self.cur_position?;

        let next_pos = self.hash_chain.hash_chain[position as usize % HASH_CHAIN_MODULO];
        self.cur_position =
            if next_pos == u32::MAX || self.orig_position - next_pos > LONG_OFFSET_MAX {
                None
            } else {
                Some(next_pos)
            };

        self.cur_position
    }
}

pub(crate) struct HashChainPrefixSearcher<'a> {
    buffer: &'a [u8],
    hash_chain: HashChain,
}

impl<'a> PrefixSearcher<'a> for HashChainPrefixSearcher<'a> {
    fn build(buffer: &'a [u8]) -> Self {
        let mut hash_chain = HashChain::new(buffer.len());

        let _ = hash_chain.insert(prefix(buffer), 0);

        Self { buffer, hash_chain }
    }

    fn search<F: FnMut(usize, usize, usize)>(&mut self, pos: usize, mut found_fn: F) {
        let mut min_length = 2;
        self.hash_chain
            .insert(prefix(&self.buffer[pos..]), pos as u32)
            .take_while(|found_pos| pos as u32 - found_pos <= LONG_OFFSET_MAX)
            .for_each(|found_pos| {
                let match_length = match_length(
                    self.buffer,
                    pos,
                    found_pos as usize,
                    LONG_LENGTH_MAX as usize,
                    3,
                );
                if match_length > min_length {
                    found_fn(found_pos as usize, min_length + 1, match_length + 1);
                    min_length = match_length;
                }
            });
    }
}
