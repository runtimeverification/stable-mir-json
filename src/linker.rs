// linker.rs

extern crate stable_mir;

use stable_mir::ty::{Ty, IndexedVal};

use crate::printer::{SmirJson, LinkMapKey, TypeMetadata};

/// Determine the range of `Ty`s used in this SmirJson
fn ty_range(smir: SmirJson) -> usize {
    let f_max: usize = smir.functions.iter()
        .map(|(k, _)| k.0.to_index())
        .max()
        .unwrap_or(0);

    let ty_max: usize = smir.types.iter()
        .map(|(ty, _)| ty.to_index())
        .max()
        .unwrap_or(0);

    std::cmp::max(f_max, ty_max)
}

/// modifies the given SmirJson by adding offset to all `Ty` used within
fn apply_offset(smir: &mut SmirJson, o: usize) {

    smir.functions = smir.functions.iter()
        .map(|(LinkMapKey(ty, lmk), name)| (LinkMapKey(offset(*ty, o), *lmk), name.clone()))
        .collect();

    smir.types = smir.types.iter()
        .map(|(ty, info)| (offset(*ty, o), offset_type(info.clone(), o)))
        .collect();

    smir.items = todo!();
}

fn offset_type(info: TypeMetadata, o: usize) -> TypeMetadata {
    use TypeMetadata::*;
    match info {
        PrimitiveType(_) => info,
        EnumType { name:_ , adt_def: _, discriminants: _ } => info,
        StructType { name, adt_def, fields } =>
            StructType { name, adt_def, fields: fields.into_iter().map(|ty| offset(ty, o)).collect()},
        UnionType { name: _, adt_def: _ } => info,
        ArrayType(ty, opt_length) => ArrayType(offset(ty, o), opt_length),
        PtrType(ty) => PtrType(offset(ty, o)),
        RefType(ty) => RefType(offset(ty, o)),
        TupleType { types } => TupleType { types: types.into_iter().map(|ty| offset(ty, o)).collect() },
        FunType(_) => info,
    }
}

fn offset(ty: Ty, o: usize) -> Ty {
        Ty::to_val(ty.to_index() + o)
}
