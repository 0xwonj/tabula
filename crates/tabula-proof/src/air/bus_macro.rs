//! Macro for generating typed bus extension traits from schema declarations.
//!
//! The [`define_bus!`] macro generates a trait + blanket impl for each bus,
//! encoding the tuple schema (field order, width, `InteractionKind`) once.

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
                $kind:expr,
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
                    #[allow(clippy::vec_init_then_push)]
                    let values = {
                        let mut v: Vec<<Self as p3_air::AirBuilder>::Expr> = Vec::new();
                        $( $crate::define_bus!(@push v, $field_name, $field_kind $( $field_param )? ); )+
                        v
                    };
                    self.send($crate::air::interaction::AirInteraction {
                        values,
                        multiplicity: mult,
                        kind: $kind,
                    });
                }

                fn $recv_name(
                    &mut self,
                    $( $field_name : $crate::define_bus!(@param_type $field_kind $( $field_param )? ), )+
                    mult: Self::Expr,
                ) {
                    #[allow(clippy::vec_init_then_push)]
                    let values = {
                        let mut v: Vec<<Self as p3_air::AirBuilder>::Expr> = Vec::new();
                        $( $crate::define_bus!(@push v, $field_name, $field_kind $( $field_param )? ); )+
                        v
                    };
                    self.receive($crate::air::interaction::AirInteraction {
                        values,
                        multiplicity: mult,
                        kind: $kind,
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
                $kind:expr,
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
                    #[allow(clippy::vec_init_then_push)]
                    let values = {
                        let mut v: Vec<<Self as p3_air::AirBuilder>::Expr> = Vec::new();
                        $( $crate::define_bus!(@push v, $field_name, $field_kind $( $field_param )? ); )+
                        v
                    };
                    self.send($crate::air::interaction::AirInteraction {
                        values,
                        multiplicity: mult,
                        kind: $kind,
                    });
                }
            }
        )*
    };

    // ── @param_type: map field kind to Rust type ──

    (@param_type expr) => { Self::Expr };
    (@param_type var_arr $n:literal) => { &[Self::Var; $n] };
    (@param_type var_slice) => { &[Self::Var] };
    (@param_type u64limbs) => { &$crate::air::gadgets::U64Limbs<Self::Var> };
    (@param_type access_tuple) => { $crate::air::bus::AccessTupleExpr<Self::Expr> };

    // ── @push: pack a field value into the Vec<Expr> ──

    (@push $vec:ident, $f:ident, expr) => { $vec.push($f); };
    (@push $vec:ident, $f:ident, var_arr $n:literal) => {
        for _v in $f { $vec.push(_v.clone().into()); }
    };
    (@push $vec:ident, $f:ident, var_slice) => {
        for _v in $f { $vec.push(_v.clone().into()); }
    };
    (@push $vec:ident, $f:ident, u64limbs) => {
        $vec.push($f.limb0.clone().into());
        $vec.push($f.limb1.clone().into());
        $vec.push($f.limb2.clone().into());
    };
    (@push $vec:ident, $f:ident, access_tuple) => {
        $vec.extend($f.into_values());
    };
}
