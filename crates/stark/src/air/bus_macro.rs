//! Macro for generating typed bus extension traits from schema declarations.
//!
//! The [`define_bus!`] macro generates a trait + blanket impl for each bus,
//! encoding the tuple schema (field order, width, [`BusId`]) once.

/// Define one or more LogUp bus extension traits.
///
/// Each bus declaration generates:
/// - A public trait `{Name}AirBuilder: InteractionAirBuilder`
/// - Typed `send_*()` / `receive_*()` methods
/// - A blanket impl on all `InteractionAirBuilder` types
///
/// # Field types
///
/// - `field: expr` — a single `AB::Expr`
/// - `field: var_arr<N>` — a `&[AB::Var; N]` array
/// - `field: var_slice` — a `&[AB::Var]` slice
/// - `field: u64limbs` — a `&U64Limbs<AB::Var>` (3 limbs)
/// - `field: access_tuple` — an `AccessTupleExpr<AB::Expr>`
#[macro_export]
macro_rules! define_bus {
    // ── Entry: send + receive bus ──
    (
        $(
            $(#[$bus_meta:meta])*
            pub $trait_name:ident (
                $bus_id:expr,
                $send_name:ident,
                $recv_name:ident
            ) {
                $( $field_name:ident : $field_kind:ident $( < $field_param:literal > )? ),+ $(,)?
            }
        )*
    ) => {
        $(
            $(#[$bus_meta])*
            pub trait $trait_name: $crate::air::builder::InteractionAirBuilder {
                /// Send on this bus.
                fn $send_name(
                    &mut self,
                    $( $field_name : $crate::define_bus!(@param_type $field_kind $( $field_param )? ), )+
                    mult: Self::Expr,
                );

                /// Receive on this bus.
                fn $recv_name(
                    &mut self,
                    $( $field_name : $crate::define_bus!(@param_type $field_kind $( $field_param )? ), )+
                    mult: Self::Expr,
                );
            }

            impl<AB: $crate::air::builder::InteractionAirBuilder> $trait_name for AB {
                fn $send_name(
                    &mut self,
                    $( $field_name : $crate::define_bus!(@param_type $field_kind $( $field_param )? ), )+
                    mult: Self::Expr,
                ) {
                    let values: Vec<<Self as p3_air::AirBuilder>::Expr> =
                        [$( $crate::define_bus!(@to_vec $field_name, $field_kind $( $field_param )? ) ),+].concat();
                    self.send($crate::air::interaction::AirInteraction {
                        values,
                        multiplicity: mult,
                        bus: $bus_id,
                    });
                }

                fn $recv_name(
                    &mut self,
                    $( $field_name : $crate::define_bus!(@param_type $field_kind $( $field_param )? ), )+
                    mult: Self::Expr,
                ) {
                    let values: Vec<<Self as p3_air::AirBuilder>::Expr> =
                        [$( $crate::define_bus!(@to_vec $field_name, $field_kind $( $field_param )? ) ),+].concat();
                    self.receive($crate::air::interaction::AirInteraction {
                        values,
                        multiplicity: mult,
                        bus: $bus_id,
                    });
                }
            }
        )*
    };

    // ── Entry: send-only bus ──
    (
        $(
            $(#[$bus_meta:meta])*
            pub $trait_name:ident (
                $bus_id:expr,
                send_only $send_name:ident
            ) {
                $( $field_name:ident : $field_kind:ident $( < $field_param:literal > )? ),+ $(,)?
            }
        )*
    ) => {
        $(
            $(#[$bus_meta])*
            pub trait $trait_name: $crate::air::builder::InteractionAirBuilder {
                /// Send on this bus.
                fn $send_name(
                    &mut self,
                    $( $field_name : $crate::define_bus!(@param_type $field_kind $( $field_param )? ), )+
                    mult: Self::Expr,
                );
            }

            impl<AB: $crate::air::builder::InteractionAirBuilder> $trait_name for AB {
                fn $send_name(
                    &mut self,
                    $( $field_name : $crate::define_bus!(@param_type $field_kind $( $field_param )? ), )+
                    mult: Self::Expr,
                ) {
                    let values: Vec<<Self as p3_air::AirBuilder>::Expr> =
                        [$( $crate::define_bus!(@to_vec $field_name, $field_kind $( $field_param )? ) ),+].concat();
                    self.send($crate::air::interaction::AirInteraction {
                        values,
                        multiplicity: mult,
                        bus: $bus_id,
                    });
                }
            }
        )*
    };

    // ── @param_type: map field kind to Rust type ──

    (@param_type expr) => { Self::Expr };
    (@param_type var_arr $n:literal) => { &[Self::Var; $n] };
    (@param_type var_slice) => { &[Self::Var] };
    (@param_type u64limbs) => { &$crate::air::primitives::U64Limbs<Self::Var> };
    (@param_type access_tuple) => { $crate::air::bus::AccessTupleExpr<Self::Expr> };

    // ── @to_vec: convert a field value to Vec<Expr> ──

    (@to_vec $f:ident, expr) => { vec![$f] };
    (@to_vec $f:ident, var_arr $n:literal) => {
        $f.iter().map(|_v| _v.clone().into()).collect::<Vec<_>>()
    };
    (@to_vec $f:ident, var_slice) => {
        $f.iter().map(|_v| _v.clone().into()).collect::<Vec<_>>()
    };
    (@to_vec $f:ident, u64limbs) => {
        vec![$f.limb0.clone().into(), $f.limb1.clone().into(), $f.limb2.clone().into()]
    };
    (@to_vec $f:ident, access_tuple) => {
        $f.into_values()
    };
}
