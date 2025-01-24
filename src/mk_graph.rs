use std::{collections::HashMap, hash::{DefaultHasher, Hash, Hasher}};

extern crate stable_mir;

use stable_mir::ty::Ty;
use stable_mir::mir::{
  BasicBlock,
  ConstOperand,
  Operand,
  Place,
  Statement,
  TerminatorKind,
  UnwindAction,
};

use crate::{printer::{FnSymType, SmirJson}, MonoItemKind};

use dot_writer::{DotWriter, Attributes, Scope};

impl SmirJson<'_> {

  pub fn to_dot_file(self) -> String {
    let mut bytes = Vec::new();

    { 
      let mut writer = DotWriter::from(&mut bytes);

      writer.set_pretty_print(true);

      let mut graph = writer.digraph();
      graph.set_label(&self.name[..]);

      let func_map: HashMap<Ty, String> = 
        self.functions
          .into_iter()
          .map(|(k,v)| (k.0, function_string(v)))
          .collect();

      for item in self.items {
        match item.mono_item_kind {
          MonoItemKind::MonoItemFn{ name, body, id: _} => {
            let mut c = graph.cluster();
            c.set_label(&name[..]);

            // Cannot define local functions that capture env. variables. Instead we define _closures_.
            let process_block = |cluster:&mut Scope<'_,'_>, node_id: usize, b: &BasicBlock | {
              let name = &item.symbol_name;
            //fn process_block<'a,'b>(name: &String, cluster:&mut Scope<'a,'b>, node_id: usize, b: &BasicBlock) {
              let this_block = block_name(name, node_id);
              let mut n = cluster.node_named(&this_block);
              // TODO: render statements and terminator as text label (with line breaks)
              // switch on terminator kind, add inner and out-edges according to terminator
              use TerminatorKind::*;
              match &b.terminator.kind {

                Goto{target} => {
                  n.set_label("Goto");
                  drop(n); // so we can borro `cluster` again below
                  cluster.edge(&this_block, block_name(name, *target));
                },
                SwitchInt{discr:_, targets} => {
                  n.set_label("SwitchInt");
                  drop(n); // so we can borrow `cluster` again below
                  for (d,t) in targets.clone().branches() {
                    cluster
                      .edge(&this_block, block_name(name, t))
                      .attributes()
                      .set_label(&format!("{d}"));
                  }
                },
                Resume{} => {
                  n.set_label("Resume"); 
                },
                Abort{} => {
                  n.set_label("Abort");
                },
                Return{} => {
                  n.set_label("Return");
                },
                Unreachable{} => {
                  n.set_label("Unreachable");
                },
                TerminatorKind::Drop{place, target, unwind} => {
                  n.set_label(&format!("Drop {}", show_place(place)));
                  drop(n);
                  if let UnwindAction::Cleanup(t) = unwind {
                    cluster
                      .edge(&this_block, block_name(name, *t))
                      .attributes()
                      .set_label("Cleanup");
                  }
                  cluster
                    .edge(&this_block, block_name(name, *target));
                },
                Call{func, args, destination, target, unwind} => {
                  n.set_label(&format!("Call()"));
                  drop(n);
                  if let UnwindAction::Cleanup(t) = unwind {
                    cluster
                      .edge(&this_block, block_name(name, *t))
                      .attributes()
                      .set_label("Cleanup");
                  }
                  if let Some(t) = target {
                    let dest = show_place(destination);
                    cluster
                      .edge(&this_block, block_name(name, *t))
                      .attributes()
                      .set_label(&dest);
                  }
                  let e = match func {
                    Operand::Constant(ConstOperand{const_, ..}) => {
                      if let Some(callee) = func_map.get(&const_.ty()) {
                        cluster
                          .edge(&this_block, block_name(callee, 0))
                      } else {
                        let unknown = format!("{}", const_.ty());
                        cluster
                          .edge(&this_block, unknown)
                      }
                    },
                    Operand::Copy(place) => {
                      cluster.edge(&this_block, format!("{}: {}", &this_block, show_place(place)))
                    },
                    Operand::Move(place) => {
                      cluster.edge(&this_block,  format!("{}: {}", &this_block, show_place(place)))
                    },
                  };
                  let arg_str = args.into_iter().map(show_op).collect::<Vec<String>>().join(",");
                  e.attributes().set_label(&arg_str);

                },
                Assert{target, ..} => {
                  n.set_label("Assert");
                  drop(n);
                  cluster
                    .edge(&this_block, block_name(name, *target));
                },
                InlineAsm{destination, unwind,..} => {
                  n.set_label("Inline ASM");
                  drop(n);
                  if let Some(t) = destination {
                    cluster
                    .edge(&this_block, block_name(name, *t));
                  }
                  if let UnwindAction::Cleanup(t) = unwind {
                    cluster
                      .edge(&this_block, block_name(name, *t))
                      .attributes()
                      .set_label("Cleanup");
                  }
                }
              }
            };

            let process_blocks = |cluster:&mut Scope<'_,'_>, offset, blocks: &Vec<BasicBlock>| {
              let mut n:usize = offset;
              for b in blocks {
                process_block(cluster, n, b);
                n += 1;
              }
            };

            match body.len() {
              0 => {
                c.node_auto().set_label("<empty body>");
              },
              1 => {
                process_blocks(&mut c, 0, &body[0].blocks);
              }
              _more => {
                let mut curr: usize = 0;
                for b in body {
                  let mut cc = c.cluster();
                  process_blocks(&mut cc, curr, &b.blocks);
                  curr += b.blocks.len();
                }
              }
            }

          }
          MonoItemKind::MonoItemGlobalAsm { asm } => {
            let mut n = graph.node_named(short_name(&asm));
            n.set_label(&asm.lines().collect::<String>()[..]);
          }
          MonoItemKind::MonoItemStatic { name, id: _, allocation: _ } => {
            let mut n = graph.node_named(short_name(&name));
            n.set_label(&name[..]);
          }
        }

      }

    }

    String::from_utf8(bytes).expect("Error converting dot file")
  }

}

fn show_op(op: &Operand) -> String {
  match op {
    Operand::Constant(ConstOperand{const_, ..}) => format!("const :: {}", const_.ty()),
    Operand::Copy(place) => show_place(place),
    Operand::Move(place) => show_place(place),
  }
}

fn show_place(p: &Place) -> String {
  format!("_{}{}", p.local, if p.projection.len() > 0 { "(...)"} else {""})
}

fn function_string(f: FnSymType) -> String {
  match f {
    FnSymType::NormalSym(name) => name,
    FnSymType::NoOpSym(name) => format!("NoOp: {name}"),
    FnSymType::IntrinsicSym(name) => format!("Intr: {name}"),
  }
}

/// consistently naming function clusters
fn short_name(function_name: &String) -> String {
  let mut h = DefaultHasher::new();
  function_name.hash(&mut h);
  format!("X{:x}", h.finish())
}

/// consistently naming block nodes in function clusters
fn block_name(function_name: &String, id: usize) -> String {
  let mut h = DefaultHasher::new();
  function_name.hash(&mut h);
  format!("X{:x}_{}", h.finish(), id)
}
