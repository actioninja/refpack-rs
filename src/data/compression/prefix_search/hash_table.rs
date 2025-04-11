use std::collections::BTreeMap;

const SMALL_TABLE_CUTOFF: usize = 8192;

// Optimization trick from libflate_lz77
// Faster lookups for very large tables
#[derive(Debug)]
pub(crate) enum PrefixTable {
    Small(BTreeMap<u32, u32>),
    Large(LargePrefixTable),
}

impl PrefixTable {
    pub(crate) fn new(bytes: usize) -> Self {
        if bytes < SMALL_TABLE_CUTOFF {
            PrefixTable::Small(BTreeMap::new())
        } else {
            PrefixTable::Large(LargePrefixTable::new())
        }
    }

    pub(crate) fn insert(&mut self, prefix: [u8; 3], position: u32) -> Option<u32> {
        match *self {
            PrefixTable::Small(ref mut table) => {
                let prefix =
                    ((prefix[0] as u32) << 16) | ((prefix[1] as u32) << 8) | (prefix[2] as u32);
                table.insert(prefix, position)
            }
            PrefixTable::Large(ref mut table) => table.insert(prefix, position),
        }
    }
}

#[derive(Debug)]
pub(crate) struct LargePrefixTable {
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
        for &mut (key, ref mut value) in &mut *positions {
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
