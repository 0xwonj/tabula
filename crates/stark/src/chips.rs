//! Core chip identification types.
//!
//! [`ChipId`], [`ChipSpec`], and [`core_chips`] define the chip identification
//! framework. Chip implementations live in downstream crates (e.g. `tabula-chips`).

/// Default value encoding width for core chip instantiation.
///
/// All core chips use `W = DEFAULT_VALUE_WIDTH` as the const generic parameter.
/// This corresponds to the U64/I64 encoding (30+30+4 bit split → 3 KoalaBear limbs).
/// Application chips may use different widths via [`EncodingWidth`](crate::trace::EncodingWidth).
pub const DEFAULT_VALUE_WIDTH: usize = 3;

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

    /// Core chip IDs (execution + root + bus consumers), for iteration and validation.
    ///
    /// Note: ordering is canonical (by chip ID), not registration order.
    /// Shard chip IDs (100+) are allocated dynamically per column proof.
    pub const ALL: [ChipId; 6] = [
        EXECUTION,
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
            4 => Some("Poseidon"),
            5 => Some("RangeCheck"),
            6 => Some("StaticTable"),
            7 => Some("SmtColPath"),
            8 => Some("SmtTablePath"),
            _ => None,
        }
    }
}

// ── ChipId Allocator ─────────────────────────────────────────────────────

/// Sequential [`ChipId`] allocator for shard chips.
///
/// Eliminates magic offset constants by allocating unique IDs on demand.
/// Core chips use IDs 0–99; the default shard allocator starts at 100.
///
/// # Example
///
/// ```
/// use tabula_stark::chips::ChipIdAllocator;
///
/// let mut alloc = ChipIdAllocator::for_shards();
/// let id1 = alloc.next(); // ChipId(100)
/// let id2 = alloc.next(); // ChipId(101)
/// assert_ne!(id1, id2);
/// ```
pub struct ChipIdAllocator {
    next_id: u16,
}

impl ChipIdAllocator {
    /// Create an allocator starting at the given ID.
    pub fn new(start: u16) -> Self {
        Self { next_id: start }
    }

    /// Create an allocator for shard chips (starts at 100, after core chips).
    pub fn for_shards() -> Self {
        Self::new(100)
    }

    /// Allocate the next available [`ChipId`].
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> ChipId {
        let id = ChipId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Current next ID (for inspection/testing).
    pub fn peek(&self) -> u16 {
        self.next_id
    }
}

/// Metadata and capability interface for AIR chips.
///
/// [`ChipId`] is the primary identifier, used by the [`ChipRegistry`](crate)
/// for dispatch in the prover/verifier.
///
/// `Send + Sync` enables parallel proving across chips.
pub trait ChipSpec: Send + Sync {
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
