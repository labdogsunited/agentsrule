//! Philox-4x32-10 and the draw naming of spec §10.2.

const M0: u32 = 0xD251_1F53;
const M1: u32 = 0xCD9E_8D57;
const W0: u32 = 0x9E37_79B9;
const W1: u32 = 0xBB67_AE85;

/// Pass numbers (spec §10.2).
pub const ARBITRATE_ORIGIN: u32 = 1;
pub const ARBITRATE_DEST: u32 = 2;
pub const MUTATE: u32 = 3;
pub const REWRITE: u32 = 4;
/// Genesis placement (spec §10.2).
pub const GENESIS: u32 = 5;

/// Stream domains (spec §10.2).
pub const DOMAIN_ENTITY: u32 = 0;
pub const DOMAIN_CELL: u32 = 1;
pub const DOMAIN_RULE_CELL: u32 = 2;
/// A cell of a state field (`proposals/field-rules.md` step 1); `aux` is the genesis statement.
pub const DOMAIN_STATE_CELL: u32 = 3;

pub fn philox4x32_10(mut ctr: [u32; 4], mut key: [u32; 2]) -> [u32; 4] {
    for round in 0..10 {
        if round > 0 {
            key[0] = key[0].wrapping_add(W0);
            key[1] = key[1].wrapping_add(W1);
        }
        let p0 = M0 as u64 * ctr[0] as u64;
        let p1 = M1 as u64 * ctr[2] as u64;
        let (hi0, lo0) = ((p0 >> 32) as u32, p0 as u32);
        let (hi1, lo1) = ((p1 >> 32) as u32, p1 as u32);
        ctr = [hi1 ^ ctr[1] ^ key[0], lo1, hi0 ^ ctr[3] ^ key[1], lo0];
    }
    ctr
}

/// Where a draw comes from: everything but its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stream {
    pub seed: u64,
    pub tick: u32,
    pub stream: u32,
    pub domain: u32,
    pub aux: u32,
    pub pass: u32,
}

impl Stream {
    /// Draw number `index` of this stream.
    pub fn draw(&self, index: u64) -> u32 {
        debug_assert!(self.pass < 256 && self.domain < 256 && self.aux < 65_536);
        let key = [self.seed as u32, (self.seed >> 32) as u32];
        let ctr = [(index / 4) as u32, self.tick, self.stream, (self.pass << 24) | (self.domain << 16) | self.aux];
        philox4x32_10(ctr, key)[(index % 4) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Known-answer vectors from the Random123 distribution (kat_vectors).
    #[test]
    fn known_answers() {
        assert_eq!(philox4x32_10([0; 4], [0; 2]), [0x6627_e8d5, 0xe169_c58d, 0xbc57_ac4c, 0x9b00_dbd8]);
        assert_eq!(
            philox4x32_10([u32::MAX; 4], [u32::MAX; 2]),
            [0x408f_276d, 0x41c8_3b0e, 0xa20b_c7c6, 0x6d54_51fd]
        );
        assert_eq!(
            philox4x32_10([0x243f_6a88, 0x85a3_08d3, 0x1319_8a2e, 0x0370_7344], [0xa409_3822, 0x299f_31d0]),
            [0xd16c_fe09, 0x94fd_cceb, 0x5001_e420, 0x2412_6ea1]
        );
    }

    #[test]
    fn domains_separate_entities_from_cells() {
        let s = Stream { seed: 7, tick: 3, stream: 5, domain: DOMAIN_ENTITY, aux: 0, pass: ARBITRATE_ORIGIN };
        let c = Stream { domain: DOMAIN_CELL, ..s };
        assert_ne!(s.draw(0), c.draw(0));
    }
}
