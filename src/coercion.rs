extern crate rustc_middle;

use rustc_middle::ty::Ty;

#[derive(Debug)]
pub struct CoercionBase<'tcx> {
    pub src_ty: Ty<'tcx>,
    pub dst_ty: Ty<'tcx>,
}