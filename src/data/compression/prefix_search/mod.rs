pub(crate) mod hash_chain;
pub(crate) mod hash_table;
pub(crate) mod multi_level_hash_chain;

pub fn prefix(input_buf: &[u8]) -> [u8; 3] {
    let buf: &[u8] = &input_buf[..3];
    [buf[0], buf[1], buf[2]]
}

pub trait PrefixSearcher<'a> {
    fn build(buffer: &'a [u8]) -> Self;
    fn search<F: FnMut(usize, usize, usize)>(&mut self, pos: usize, found_fn: F);
}

// optimization: we only have to reserve LONG_OFFSET_MAX + 1 bytes
// but since it costs less instructions to do modulo by a power of two
// we'll use the next largest power of two
// const HASH_CHAIN_MODULO: usize = (LONG_OFFSET_MAX + 1) as usize;
const HASH_CHAIN_MODULO: usize = 1 << 18;
