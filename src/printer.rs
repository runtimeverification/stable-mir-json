use std::io;
extern crate rustc_middle;
extern crate rustc_smir;
extern crate stable_mir;
use rustc_middle::ty::TyCtxt;
use rustc_smir::rustc_internal;
use stable_mir::CrateDef;

pub fn print_item(tcx: TyCtxt<'_>, item: &stable_mir::CrateItem, out: &mut io::Stdout) {
  let kind = item.kind();
  let _ = item.emit_mir(out);
  println!("{:?}", item.body());
  for (idx, promoted) in tcx.promoted_mir(rustc_internal::internal(tcx,item.def_id())).into_iter().enumerate() {
    let promoted_body = rustc_internal::stable(promoted);
    let _ = promoted_body.dump(out, format!("promoted[{}:{}]", item.name(), idx).as_str());
    println!("{:?}", promoted_body);
  }
}

pub fn print_all_items(tcx: TyCtxt<'_>) {
  let mut out = io::stdout();
  for item in stable_mir::all_local_items().iter() {
    print_item(tcx, item, &mut out);
  }
}

