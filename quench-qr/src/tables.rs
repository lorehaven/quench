//! The per-version, per-level constants of ISO 18004: how many error-correction codewords a block has and how
//! many blocks the symbol is cut into. Everything else (capacity, block sizes) follows from them and from the
//! module count of the version.

/// How much of the symbol can be lost and still decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcLevel {
    /// About 7%.
    L,
    /// About 15%.
    M,
    /// About 25%.
    Q,
    /// About 30%.
    H,
}

impl EcLevel {
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::L => 0,
            Self::M => 1,
            Self::Q => 2,
            Self::H => 3,
        }
    }

    /// The two format-information bits that name the level.
    pub(crate) const fn format_bits(self) -> u32 {
        match self {
            Self::L => 1,
            Self::M => 0,
            Self::Q => 3,
            Self::H => 2,
        }
    }
}

/// Error-correction codewords per block, indexed `[level][version]` (index 0 unused).
const ECC_PER_BLOCK: [[u8; 41]; 4] = [
    [
        0, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28,
        30, 30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
    ],
    [
        0, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28,
        28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28,
    ],
    [
        0, 13, 22, 18, 26, 18, 24, 18, 22, 20, 24, 28, 26, 24, 20, 30, 24, 28, 28, 26, 30, 28, 30,
        30, 30, 30, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
    ],
    [
        0, 17, 28, 22, 16, 22, 28, 26, 26, 24, 28, 24, 28, 22, 24, 24, 30, 28, 28, 26, 28, 30, 24,
        30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
    ],
];

/// Number of blocks, indexed `[level][version]` (index 0 unused).
const BLOCKS: [[u8; 41]; 4] = [
    [
        0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12, 13,
        14, 15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25,
    ],
    [
        0, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21,
        23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49,
    ],
    [
        0, 1, 1, 2, 2, 4, 4, 6, 6, 8, 8, 8, 10, 12, 16, 12, 17, 16, 18, 21, 20, 23, 23, 25, 27, 29,
        34, 34, 35, 38, 40, 43, 45, 48, 51, 53, 56, 59, 62, 65, 68,
    ],
    [
        0, 1, 1, 2, 4, 4, 4, 5, 6, 8, 8, 11, 11, 16, 16, 18, 16, 19, 21, 25, 25, 25, 34, 30, 32,
        35, 37, 40, 42, 45, 48, 51, 54, 57, 60, 63, 66, 70, 74, 77, 81,
    ],
];

pub const fn ecc_per_block(version: u8, level: EcLevel) -> usize {
    ECC_PER_BLOCK[level.index()][version as usize] as usize
}

pub const fn blocks(version: u8, level: EcLevel) -> usize {
    BLOCKS[level.index()][version as usize] as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[quench_test::test]
    fn levels_name_themselves() {
        let all = [EcLevel::L, EcLevel::M, EcLevel::Q, EcLevel::H];
        assert_eq!(all.map(EcLevel::index), [0, 1, 2, 3]);
        assert_eq!(all.map(EcLevel::format_bits), [1, 0, 3, 2]);
    }

    #[quench_test::test]
    fn the_tables_are_looked_up_by_version_and_level() {
        assert_eq!(
            (ecc_per_block(1, EcLevel::L), blocks(1, EcLevel::L)),
            (7, 1)
        );
        assert_eq!(
            (ecc_per_block(40, EcLevel::H), blocks(40, EcLevel::H)),
            (30, 81)
        );
        assert_eq!(
            (ecc_per_block(10, EcLevel::Q), blocks(10, EcLevel::Q)),
            (24, 8)
        );
    }
}
