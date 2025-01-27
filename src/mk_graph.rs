use std::{collections::{HashMap, HashSet}, fs::File, hash::{DefaultHasher, Hash, Hasher}, io::{self, Write}};

use dot_writer::{Attributes, Color, DotWriter, Scope, Style};

extern crate rustc_middle;
use rustc_middle::ty::TyCtxt;

extern crate stable_mir;
use rustc_session::config::{OutFileName, OutputType};

extern crate rustc_session;
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

use crate::{printer::{FnSymType, SmirJson, collect_smir}, MonoItemKind};

// entry point to write the dot file
pub fn emit_dotfile(tcx: TyCtxt<'_>) {

  let smir_dot = collect_smir(tcx).to_dot_file();

  match tcx.output_filenames(()).path(OutputType::Mir) {
    OutFileName::Stdout => {
      write!(io::stdout(), "{}", smir_dot).expect("Failed to write smir.dot");
    }
    OutFileName::Real(path) => {
      let mut b =
        io::BufWriter::new(
          File::create(&path.with_extension("smir.dot"))
            .expect("Failed to create {path}.smir.dot output file"));
      write!(b, "{}", smir_dot).expect("Failed to write smir.dot");
    }
  }
}

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

      let item_names: HashSet<String> =
        self.items
          .iter()
          .map(|i| i.symbol_name.clone())
          .collect();

      // first create all nodes for functions not in the items list
      for f  in func_map.values() {
        if ! item_names.contains(f) {
          graph
            .node_named(block_name(f, 0))
            .set_label(&name_lines(f))
            .set_color(Color::Red);
        }
      }

      for item in self.items {
        match item.mono_item_kind {
          MonoItemKind::MonoItemFn{ name, body, id: _} => {
            let mut c = graph.cluster();
            c.set_label(&name_lines(&name));
            if is_unqualified(&name) {
              c.set_style(Style::Filled);
              c.set_color(Color::LightGrey);
            }

            // Cannot define local functions that capture env. variables. Instead we define _closures_.
            let process_block = |cluster:&mut Scope<'_,'_>, node_id: usize, b: &BasicBlock | {
              let name = &item.symbol_name;
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
                Call{func: _, args: _, destination, target, unwind} => {
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

                  // The call edge has to be drawn outside the cluster, and therefore outside this function!
                  // I have to separate this code into its own second function.
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

            match &body.len() {
              0 => {
                c.node_auto().set_label("<empty body>");
              },
              1 => {
                process_blocks(&mut c, 0, &body[0].blocks);
              }
              _more => {
                let mut curr: usize = 0;
                for b in &body {
                  let mut cc = c.cluster();
                  process_blocks(&mut cc, curr, &b.blocks);
                  curr += b.blocks.len();
                }
              }
            }
            drop(c); // so we can borrow graph again

            // call edges have to be added _outside_ the cluster of blocks for one function
            // because they go between different clusters. Due to a scope/borrow issue, we have
            // to make a 2nd pass over the bodies of the item.
            let add_call_edges = | graph: &mut Scope<'_,'_>, offset: usize, bs: &Vec<BasicBlock> | {
              for (i, b) in bs.iter().enumerate() {
                let this_block = block_name(&item.symbol_name, offset + i);
                
                match &b.terminator.kind {
                  TerminatorKind::Call{func, args, ..} => {
                    let e = match func {
                      Operand::Constant(ConstOperand{const_, ..}) => {
                        if let Some(callee) = func_map.get(&const_.ty()) {
                          // if ! item_names.contains(callee) {
                          //   graph
                          //     .node_named(block_name(callee, 0))
                          //     .set_label(callee);
                          // }
                          graph
                            .edge(&this_block, block_name(callee, 0))
                        } else {
                          let unknown = format!("{}", const_.ty());
                          // graph.node_named(&unknown);
                          graph
                            .edge(&this_block, unknown)
                        }
                      },
                      Operand::Copy(place) => {
                        graph.edge(&this_block, format!("{}: {}", &this_block, show_place(place)))
                      },
                      Operand::Move(place) => {
                        graph.edge(&this_block,  format!("{}: {}", &this_block, show_place(place)))
                      },
                    };
                    let arg_str = args.into_iter().map(show_op).collect::<Vec<String>>().join(",");
                    e.attributes().set_label(&arg_str);

                  },
                  _other => {
                    // nothing to do
                  },
                }
              }
            };

            match &body.len() {
              0 => {},
              1 => {
                add_call_edges(&mut graph, 0, &body[0].blocks);
              }
              _more => {
                let mut curr: usize = 0;
                for b in &body {
                  add_call_edges(&mut graph, curr, &b.blocks);
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

fn is_unqualified(name: &String) -> bool {
  ! name.contains("::")
}

fn function_string(f: FnSymType) -> String {
  match f {
    FnSymType::NormalSym(name) => name,
    FnSymType::NoOpSym(name) => format!("NoOp: {name}"),
    FnSymType::IntrinsicSym(name) => format!("Intr: {name}"),
  }
}

fn name_lines(name: &String) -> String {
  name
    .split_inclusive(" ")
    .flat_map(|s| s.split_inclusive("::"))
    .map(|s| s.to_string())
    .collect::<Vec<String>>()
    .join("\\n")
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
