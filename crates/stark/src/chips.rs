//! Core chip identification types.
//!
//! [`ChipId`], [`ChipSpec`], and [`core_chips`] define the chip identification
//! framework. Chip implementations live in downstream crates (e.g. `tabula-chips`).

/// Open chip identifier for the proof system.
///
/// Unlike a closed enum, `ChipId` is a transparent newtype that allows
/// downstream crates to define new chips without modifying Tabula.
/// Core chip IDs are defined in [`core_chips`]. Application-specific
/// chips should use IDs >= 100 to avoid collisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChipId(pub u16);

impl ChipId {
    /// Integer tag for diagnostics and serialization.
    pub const fn tag(self) -> u16 {
        self.0
    }
}

impl std::fmt::Display for ChipId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = core_chips::name(self) {
            f.write_str(name)
        } else {
            write!(f, "Chip({})", self.0)
        }
    }
}

/// Core chip identifiers for the Tabula proof system.
///
/// Each constant corresponds to exactly one chip type in the default
/// chip set. Application-specific chips should use IDs >= 100.
pub mod core_chips {
    use super::ChipId;

    /// Execution chip (instruction interpreter).
    pub const EXECUTION: ChipId = ChipId(0);
    /// Inter-transaction ordering chip.
    pub const INTER_TX_ORDER: ChipId = ChipId(1);
    /// State column chip (per-column sorted memory).
    pub const STATE_COLUMN: ChipId = ChipId(2);
    /// Column metadata chip (commitment wiring).
    pub const COLUMN_META: ChipId = ChipId(3);
    /// Poseidon2 permutation chip.
    pub const POSEIDON: ChipId = ChipId(4);
    /// Range check preprocessed table.
    pub const RANGE_CHECK: ChipId = ChipId(5);
    /// Static table lookup chip.
    pub const STATIC_TABLE: ChipId = ChipId(6);
    /// SMT column-level path chip.
    pub const SMT_COL_PATH: ChipId = ChipId(7);
    /// SMT table-level path chip.
    pub const SMT_TABLE_PATH: ChipId = ChipId(8);

    /// All core chip IDs, for iteration and validation.
    pub const ALL: [ChipId; 9] = [
        EXECUTION,
        INTER_TX_ORDER,
        STATE_COLUMN,
        COLUMN_META,
        POSEIDON,
        RANGE_CHECK,
        STATIC_TABLE,
        SMT_COL_PATH,
        SMT_TABLE_PATH,
    ];

    /// Human-readable name for a core chip ID, or `None` for app-defined chips.
    pub const fn name(id: &ChipId) -> Option<&'static str> {
        match id.0 {
            0 => Some("Execution"),
            1 => Some("InterTxOrder"),
            2 => Some("StateColumn"),
            3 => Some("ColumnMeta"),
            4 => Some("Poseidon"),
            5 => Some("RangeCheck"),
            6 => Some("StaticTable"),
            7 => Some("SmtColPath"),
            8 => Some("SmtTablePath"),
            _ => None,
        }
    }
}

/// Metadata and capability interface for AIR chips.
///
/// [`ChipId`] is the primary identifier, used by [`crate::air::chip_set::ChipSet`]
/// for typed dispatch in the prover/verifier.
///
/// The `Default` bound enables construction from ZSTs in `ChipSet::all_chips()`.
/// `Send + Sync` enables parallel proving across chips.
pub trait ChipSpec: Default + Send + Sync {
    /// Open chip identifier.
    fn chip_id(&self) -> ChipId;

    /// Human-readable chip name, derived from [`chip_id()`](Self::chip_id).
    fn chip_name(&self) -> &'static str {
        core_chips::name(&self.chip_id()).unwrap_or("Unknown")
    }

    /// Number of public values consumed by this chip (default: 0).
    fn num_public_values(&self) -> usize {
        0
    }

    /// Width of preprocessed trace columns (default: 0 = no preprocessed).
    ///
    /// A nonzero value indicates this chip has a preprocessed trace.
    /// Used by keygen to construct symbolic variables.
    fn preprocessed_width(&self) -> usize {
        0
    }

    /// Whether this chip declares any LogUp interactions (default: true).
    /// Chips with no interactions skip permutation trace generation.
    fn has_interactions(&self) -> bool {
        true
    }
}
