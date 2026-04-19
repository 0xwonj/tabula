//! Dedicated transcript lane for the proved public-context commitment.

use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_contract::format::public_statement_transcript::{
    PUBLIC_CONTEXT_TRANSCRIPT_DOMAIN_TAG, PUBLIC_STATEMENT_TRANSCRIPT_RATE,
    public_context_header_block,
};
use tabula_core::error::TabulaError;
use tabula_gadgets::constrain_is_real_prefix;
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::{borrow_cols, borrow_cols_mut, num_cols};
use tabula_stark::air::interaction::{AirInteraction, core_buses};
use tabula_stark::chips::{ChipId, ChipSpec};
use tabula_stark::trace::TraceGenerator;
use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
use tabula_stark::trace::trace_map::TraceMap;

use crate::poseidon::constants::{WIDTH, poseidon2_permutation};

/// Witness-store label for public-context transcript items.
pub const PUBLIC_CONTEXT_TRANSCRIPT_WITNESS_LABEL: &str = "public_context_transcript_items";
/// Fixed chip id for the public-context transcript lane.
pub const PUBLIC_CONTEXT_TRANSCRIPT_CHIP_ID: ChipId = ChipId(94);
/// Private bus binding execution `LoadContext` rows to the transcript lane.
pub const PUBLIC_CONTEXT_TRANSCRIPT_BUS: tabula_stark::air::interaction::BusId =
    tabula_stark::air::interaction::BusId(104);

#[repr(C)]
struct PublicContextTranscriptCols<T> {
    is_real: T,
    is_header: T,
    is_last: T,
    item_count: T,
    item_index: T,
    block_values: [T; PUBLIC_STATEMENT_TRANSCRIPT_RATE],
    prev_digest: [T; 8],
    perm_input: [T; WIDTH],
    perm_output: [T; 8],
}

const fn public_context_transcript_width() -> usize {
    num_cols::<PublicContextTranscriptCols<u8>, u8>()
}

/// Execution-tier transcript chip proving the public-context digest.
#[derive(Clone, Copy, Debug, Default)]
pub struct PublicContextTranscriptChip;

impl ChipSpec for PublicContextTranscriptChip {
    fn chip_id(&self) -> ChipId {
        PUBLIC_CONTEXT_TRANSCRIPT_CHIP_ID
    }
}

impl<F> BaseAir<F> for PublicContextTranscriptChip {
    fn width(&self) -> usize {
        public_context_transcript_width()
    }

    fn num_public_values(&self) -> usize {
        8
    }
}

impl<AB: InteractionAirBuilder> Air<AB> for PublicContextTranscriptChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local: &PublicContextTranscriptCols<AB::Var> = borrow_cols(main.current_slice());
        let next: &PublicContextTranscriptCols<AB::Var> = borrow_cols(main.next_slice());

        let is_real: AB::Expr = local.is_real.into();
        let both_real: AB::Expr = is_real.clone() * next.is_real.into();
        let is_item: AB::Expr = AB::Expr::ONE - local.is_header.into();

        builder.assert_bool(local.is_real);
        builder.assert_bool(local.is_header);
        builder.assert_bool(local.is_last);
        constrain_is_real_prefix(builder, local.is_real, next.is_real);

        builder
            .when_first_row()
            .assert_zero(is_real.clone() * (AB::Expr::ONE - local.is_header.into()));
        builder
            .when_first_row()
            .assert_zero(is_real.clone() * local.item_index.into());
        for idx in 0..8 {
            builder
                .when_first_row()
                .assert_zero(is_real.clone() * local.prev_digest[idx].into());
        }

        builder.assert_zero(is_real.clone() * local.is_header.into() * local.item_index.into());
        builder.assert_zero(
            is_real.clone()
                * local.is_header.into()
                * (local.block_values[0].into()
                    - tabula_gadgets::integer::expr_from_u32::<AB>(
                        PUBLIC_CONTEXT_TRANSCRIPT_DOMAIN_TAG,
                    )),
        );
        builder.assert_zero(
            is_real.clone()
                * local.is_header.into()
                * (local.block_values[1].into() - local.item_count.into()),
        );
        for idx in 2..PUBLIC_STATEMENT_TRANSCRIPT_RATE {
            builder.assert_zero(
                is_real.clone() * local.is_header.into() * local.block_values[idx].into(),
            );
        }

        for idx in 0..8 {
            builder.assert_zero(
                is_real.clone() * (local.perm_input[idx].into() - local.prev_digest[idx].into()),
            );
            builder.assert_zero(
                is_real.clone()
                    * (local.perm_input[8 + idx].into() - local.block_values[idx].into()),
            );
        }

        let mut poseidon_values = Vec::with_capacity(24);
        for idx in 0..16 {
            poseidon_values.push(local.perm_input[idx].into());
        }
        for idx in 0..8 {
            poseidon_values.push(local.perm_output[idx].into());
        }
        builder.send(AirInteraction {
            values: poseidon_values,
            multiplicity: is_real.clone(),
            bus: core_buses::POSEIDON_PERM,
        });

        let item_mult: AB::Expr = is_real.clone() * is_item.clone();
        let mut item_values = Vec::with_capacity(1 + PUBLIC_STATEMENT_TRANSCRIPT_RATE);
        item_values.push(local.item_index.into());
        for idx in 0..PUBLIC_STATEMENT_TRANSCRIPT_RATE {
            item_values.push(local.block_values[idx].into());
        }
        builder.receive(AirInteraction {
            values: item_values,
            multiplicity: item_mult,
            bus: PUBLIC_CONTEXT_TRANSCRIPT_BUS,
        });

        builder
            .when_transition()
            .assert_zero(is_real.clone() * local.is_last.into() * next.is_real.into());
        builder
            .when_transition()
            .assert_zero(both_real.clone() * local.is_last.into());
        builder.when_transition().assert_zero(
            is_real.clone()
                * (AB::Expr::ONE - local.is_last.into())
                * (AB::Expr::ONE - next.is_real.into()),
        );
        builder.when_transition().assert_zero(
            both_real.clone() * next.item_count.into()
                - both_real.clone() * local.item_count.into(),
        );
        builder
            .when_transition()
            .assert_zero(both_real.clone() * next.is_header.into());
        for idx in 0..8 {
            builder.when_transition().assert_zero(
                both_real.clone() * (next.prev_digest[idx].into() - local.perm_output[idx].into()),
            );
        }
        builder
            .when_transition()
            .assert_zero(both_real.clone() * local.is_header.into() * next.item_index.into());
        builder.when_transition().assert_zero(
            both_real.clone()
                * (AB::Expr::ONE - local.is_header.into())
                * (next.item_index.into() - local.item_index.into() - AB::Expr::ONE),
        );

        builder.assert_zero(
            is_real.clone()
                * (AB::Expr::ONE - local.is_header.into())
                * local.is_last.into()
                * (local.item_index.into() + AB::Expr::ONE - local.item_count.into()),
        );
        builder.assert_zero(
            is_real.clone()
                * local.is_header.into()
                * local.is_last.into()
                * local.item_count.into(),
        );

        for idx in 0..8 {
            let public_value = builder.public_values()[idx];
            builder.assert_zero(
                is_real.clone()
                    * local.is_last.into()
                    * (local.perm_output[idx].into() - public_value.into()),
            );
        }
    }
}

#[derive(Clone, Debug)]
struct PublicContextTranscriptRow {
    is_header: bool,
    is_last: bool,
    item_count: u32,
    item_index: u32,
    block_values: [KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE],
    prev_digest: [u32; 8],
    perm_input: [KoalaBear; WIDTH],
    perm_output: [u32; 8],
}

impl TraceGenerator for PublicContextTranscriptChip {
    type Input = [[KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE]];

    fn generate_trace(
        &self,
        input: &[[KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE]],
    ) -> RowMajorMatrix<KoalaBear> {
        let width = public_context_transcript_width();
        let (rows, _) = build_rows(input);
        let num_real = rows.len();
        let num_rows = (num_real + 1).next_power_of_two().max(2);
        let mut values = vec![KoalaBear::ZERO; num_rows * width];

        for (row_idx, row) in rows.iter().enumerate() {
            let offset = row_idx * width;
            let cols: &mut PublicContextTranscriptCols<KoalaBear> =
                borrow_cols_mut(&mut values[offset..offset + width]);
            cols.is_real = KoalaBear::ONE;
            cols.is_header = if row.is_header {
                KoalaBear::ONE
            } else {
                KoalaBear::ZERO
            };
            cols.is_last = if row.is_last {
                KoalaBear::ONE
            } else {
                KoalaBear::ZERO
            };
            cols.item_count = KoalaBear::new(row.item_count);
            cols.item_index = KoalaBear::new(row.item_index);
            cols.block_values = row.block_values;
            for idx in 0..8 {
                cols.prev_digest[idx] = KoalaBear::new(row.prev_digest[idx]);
                cols.perm_output[idx] = KoalaBear::new(row.perm_output[idx]);
            }
            cols.perm_input = row.perm_input;
        }

        RowMajorMatrix::new(values, width)
    }
}

impl TraceContributor for PublicContextTranscriptChip {
    fn phase(&self) -> TracePhase {
        TracePhase::INDEPENDENT
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let items = store.get::<Vec<[KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE]>>(
            PUBLIC_CONTEXT_TRANSCRIPT_WITNESS_LABEL,
        )?;
        let (rows, digest) = build_rows(items.as_slice());
        let trace = self.generate_trace(items.as_slice());
        map.insert(PUBLIC_CONTEXT_TRANSCRIPT_CHIP_ID, trace);
        map.set_public_values(
            PUBLIC_CONTEXT_TRANSCRIPT_CHIP_ID,
            digest.iter().copied().map(KoalaBear::new).collect(),
        );
        debug_assert_eq!(rows.last().map(|row| row.perm_output), Some(digest));
        Ok(())
    }
}

fn build_rows(
    items: &[[KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE]],
) -> (Vec<PublicContextTranscriptRow>, [u32; 8]) {
    let item_count = items.len() as u32;
    let mut rows = Vec::with_capacity(items.len() + 1);
    let header_block = public_context_header_block(items.len());
    let mut prev_digest = [0u32; 8];

    let mut push_row =
        |is_header: bool, is_last: bool, item_index: u32, block_values: [KoalaBear; 8]| {
            let mut perm_input = [KoalaBear::ZERO; WIDTH];
            for idx in 0..8 {
                perm_input[idx] = KoalaBear::new(prev_digest[idx]);
                perm_input[8 + idx] = block_values[idx];
            }
            let (_, output_state) = poseidon2_permutation(perm_input);
            let perm_output = core::array::from_fn(|idx| output_state[idx].as_canonical_u32());
            rows.push(PublicContextTranscriptRow {
                is_header,
                is_last,
                item_count,
                item_index,
                block_values,
                prev_digest,
                perm_input,
                perm_output,
            });
            prev_digest = perm_output;
        };

    if items.is_empty() {
        push_row(true, true, 0, header_block);
    } else {
        push_row(true, false, 0, header_block);
        for (item_index, block) in items.iter().enumerate() {
            push_row(
                false,
                item_index + 1 == items.len(),
                item_index as u32,
                *block,
            );
        }
    }

    (rows, prev_digest)
}

/// Witness kit for [`PublicContextTranscriptChip`]. Uses the
/// runtime-pre-stuff pattern: the runtime materializes the transcript
/// item vector at batch level and installs it via [`Self::insert_items`]
/// before `finalize`.
#[derive(Clone, Copy, Debug, Default)]
pub struct PublicContextTranscriptKit;

impl PublicContextTranscriptKit {
    const CHIP_ID: ChipId = PUBLIC_CONTEXT_TRANSCRIPT_CHIP_ID;

    /// Install the runtime-prepared transcript items into the kit's scratchpad.
    pub fn insert_items(
        scratch: &mut tabula_stark::witness_kit::KitScratch,
        items: Vec<[KoalaBear; 8]>,
    ) {
        scratch.insert(Self::CHIP_ID, Box::new(items));
    }
}

impl tabula_stark::witness_kit::ChipWitnessKit for PublicContextTranscriptKit {
    fn chip_id(&self) -> ChipId {
        Self::CHIP_ID
    }

    fn witness_store_label(&self) -> &'static str {
        PUBLIC_CONTEXT_TRANSCRIPT_WITNESS_LABEL
    }

    fn finalize(
        &self,
        ctx: &mut tabula_stark::witness_kit::KitFinalizeContext<'_>,
        store: &mut tabula_stark::trace::WitnessStore,
    ) -> Result<(), tabula_stark::witness_kit::KitError> {
        let items: Vec<[KoalaBear; 8]> = ctx.take_scratch_required(Self::CHIP_ID)?;
        store.put(PUBLIC_CONTEXT_TRANSCRIPT_WITNESS_LABEL, items);
        Ok(())
    }
}
