//! Type metadata construction for the SMIR JSON output.
//!
//! Translates Stable MIR `TyKind` values (along with their layouts) into
//! [`TypeMetadata`] entries that describe the structure of each type in a
//! form suitable for downstream consumers (e.g., interpreters or verifiers).

extern crate rustc_middle;
extern crate rustc_smir;
extern crate stable_mir;

use rustc_middle::ty::TyCtxt;
use rustc_smir::rustc_internal;
use stable_mir::abi::LayoutShape;
use stable_mir::ty::TyKind;

use super::schema::TypeMetadata;

pub(super) fn mk_type_metadata(
    tcx: TyCtxt<'_>,
    k: stable_mir::ty::Ty,
    t: TyKind,
    layout: Option<LayoutShape>,
) -> Option<(stable_mir::ty::Ty, TypeMetadata)> {
    use stable_mir::ty::RigidTy::*;
    use TyKind::RigidTy as T;
    use TypeMetadata::*;
    let name = format!("{k}"); // prints name with type parameters
    match t {
        T(prim_type) if t.is_primitive() => Some((k, PrimitiveType(prim_type))),
        // for enums, we need a mapping of variantIdx to discriminant
        // this requires access to the internals and is not provided as an interface function at the moment
        T(Adt(adt_def, args)) if t.is_enum() => {
            let adt_internal = rustc_internal::internal(tcx, adt_def);
            let discriminants = adt_internal
                .discriminants(tcx)
                .map(|(_, discr)| discr.val)
                .collect::<Vec<_>>();
            let fields = adt_def
                .variants()
                .iter()
                .map(|v| {
                    v.fields()
                        .iter()
                        .map(|f| f.ty_with_args(&args))
                        .collect::<Vec<stable_mir::ty::Ty>>()
                })
                .collect();
            Some((
                k,
                EnumType {
                    name,
                    adt_def,
                    discriminants,
                    fields,
                    layout,
                },
            ))
        }
        T(Adt(adt_def, args)) if t.is_struct() => {
            let fields = adt_def
                .variants()
                .pop() // NB struct, there should be a single variant
                .unwrap()
                .fields()
                .iter()
                .map(|f| f.ty_with_args(&args))
                .collect();
            Some((
                k,
                StructType {
                    name,
                    adt_def,
                    fields,
                    layout,
                },
            ))
        }
        T(Adt(adt_def, args)) if t.is_union() => {
            let fields = adt_def
                .variants()
                .pop() // TODO: Check union has single variant
                .unwrap()
                .fields()
                .iter()
                .map(|f| f.ty_with_args(&args))
                .collect();
            Some((
                k,
                UnionType {
                    name,
                    adt_def,
                    fields,
                    layout,
                },
            ))
        }
        // encode str together with primitive types
        T(Str) => Some((k, PrimitiveType(Str))),
        // for arrays and slices, record element type and optional size
        T(Array(elem_type, ty_const)) => {
            if matches!(
                ty_const.kind(),
                stable_mir::ty::TyConstKind::Unevaluated(_, _)
            ) {
                panic!("Unevaluated constant {ty_const:?} in type {k}");
            }
            Some((
                k,
                ArrayType {
                    elem_type,
                    size: Some(ty_const),
                    layout,
                },
            ))
        }

        T(Slice(elem_type)) => Some((
            k,
            ArrayType {
                elem_type,
                size: None,
                layout,
            },
        )),
        // for raw pointers and references store the pointee type
        T(RawPtr(pointee_type, _)) => Some((
            k,
            PtrType {
                pointee_type,
                layout,
            },
        )),
        T(Ref(_, pointee_type, _)) => Some((
            k,
            RefType {
                pointee_type,
                layout,
            },
        )),
        // for tuples the element types are provided
        T(Tuple(types)) => Some((k, TupleType { types, layout })),
        // opaque function types (fun ptrs, closures, FnDef) are only provided to avoid dangling ty references
        T(FnDef(_, _)) | T(FnPtr(_)) | T(Closure(_, _)) => Some((k, FunType(name))),
        // other types are not provided either
        T(Foreign(_))
        | T(Pat(_, _))
        | T(Coroutine(_, _, _))
        | T(Dynamic(_, _, _))
        | T(CoroutineWitness(_, _)) => {
            debug_log_println!(
                "\nDEBUG: Skipping unsupported ty {}: {:?}",
                k.to_index(),
                k.kind()
            );
            None
        }
        T(Never) => Some((k, VoidType)),
        TyKind::Alias(_, _) | TyKind::Param(_) | TyKind::Bound(_, _) => {
            debug_log_println!("\nSkipping undesired ty {}: {:?}", k.to_index(), k.kind());
            None
        }
        _ => {
            // redundant because of first 4 cases, but rustc does not understand that
            debug_log_println!("\nDEBUG: Funny other Ty {}: {:?}", k.to_index(), k.kind());
            None
        }
    }
}
