pub mod exact;
pub mod gen_toml;
pub mod gen_tree;
pub mod parse_compare;
pub mod recoverable;

pub struct Gen<'a> {
    data: &'a [u8],
    pub pos: usize,
}

impl<'a> Gen<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn next(&mut self) -> u8 {
        if self.pos >= self.data.len() {
            return 0;
        }
        let b = self.data[self.pos];
        self.pos += 1;
        b
    }

    pub fn pick<'t, T>(&mut self, items: &'t [T]) -> &'t T {
        let idx = self.next() as usize % items.len();
        &items[idx]
    }

    pub fn range(&mut self, lo: u8, hi: u8) -> u8 {
        lo + self.next() % (hi - lo + 1)
    }
}

pub fn pick_unique_idx<const N: usize>(g: &mut Gen<'_>, used: &mut [bool; N]) -> Option<usize> {
    let start = g.next() as usize % N;
    for i in 0..N {
        let idx = (start + i) % N;
        if !used[idx] {
            used[idx] = true;
            return Some(idx);
        }
    }
    None
}
