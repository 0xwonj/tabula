use p3_commit::{Pcs, PolynomialSpace};

use crate::config::{Challenger, EF4, TabulaPcs};

pub(crate) fn transition_opening_points(
    pcs: &TabulaPcs,
    degree_bits: usize,
    zeta: EF4,
) -> [EF4; 2] {
    let domain =
        <TabulaPcs as Pcs<EF4, Challenger>>::natural_domain_for_degree(pcs, 1 << degree_bits);
    let zeta_next = domain
        .next_point(zeta)
        .expect("domain has no next point for zeta");
    [zeta, zeta_next]
}

pub(crate) fn preprocessed_opening_points(
    pcs: &TabulaPcs,
    degree_bits: usize,
    zeta: EF4,
    uses_next_row: bool,
) -> Vec<EF4> {
    if uses_next_row {
        transition_opening_points(pcs, degree_bits, zeta).into()
    } else {
        vec![zeta]
    }
}

#[cfg(test)]
mod tests {
    use p3_field::PrimeCharacteristicRing;
    use p3_uni_stark::StarkGenericConfig;

    use super::{preprocessed_opening_points, transition_opening_points};

    #[test]
    fn local_only_preprocessed_chip_opens_only_at_zeta() {
        let config = crate::default_config();
        let pcs = config.pcs();
        let zeta = crate::EF4::ONE;

        let points = preprocessed_opening_points(pcs, 3, zeta, false);

        assert_eq!(points, vec![zeta]);
    }

    #[test]
    fn next_row_preprocessed_chip_opens_at_zeta_and_zeta_next() {
        let config = crate::default_config();
        let pcs = config.pcs();
        let zeta = crate::EF4::ONE;

        let expected = transition_opening_points(pcs, 3, zeta);
        let points = preprocessed_opening_points(pcs, 3, zeta, true);

        assert_eq!(points, expected);
        assert_eq!(points.len(), 2);
    }
}
