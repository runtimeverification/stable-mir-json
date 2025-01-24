use std::{collections::HashMap, hash::{DefaultHasher, Hash, Hasher}};

extern crate stable_mir;

use stable_mir::ty::Ty;
use stable_mir::mir::{BasicBlock, Statement, TerminatorKind};

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
          .map(|(k,v)| (k.0, mk_string(v)))
          .collect();

      for item in self.items {
        match item.mono_item_kind {
          MonoItemKind::MonoItemFn{ name, body, id: _} => {
            let mut c = graph.cluster();
            c.set_label(&name[..]);

            fn process_block<'a,'b>(name: &String, cluster:&mut Scope<'a,'b>, node_id: usize, b: &BasicBlock) {
              let mut n = cluster.node_named(block_name(name, node_id));
              // TODO: render statements and terminator as text label (with line breaks)
              n.set_label("the Terminator");
              // switch on terminator kind, add inner and out-edges according to terminator
              use TerminatorKind::*;
              match b.terminator.kind {

                Goto{target: _} => {},
                SwitchInt{discr:_, targets: _} => {},
                Resume{} => {},
                Abort{} => {},
                Return{} => {},
                Unreachable{} => {},
                TerminatorKind::Drop{place: _, target: _, unwind: _} => {},
                Call{func: _, args: _, destination: _, target: _, unwind: _} => {},
                Assert{target: _, ..} => {},
                InlineAsm{destination: _, ..} => {}
              }
            }

            fn process_blocks<'a,'b>(name: &String, cluster:&mut Scope<'a,'b>, offset: usize, blocks: &Vec<BasicBlock>) {
              let mut n = offset;
              for b in blocks {
                process_block(name, cluster, n, b);
                n += 1;
              }
            }


            match body.len() {
              0 => {
                c.node_auto().set_label("<empty body>");
              },
              1 => {
                process_blocks(&item.symbol_name, &mut c, 0, &body[0].blocks);
              }
              _more => {
                let mut curr: usize = 0;
                for b in body {
                  let mut cc = c.cluster();
                  process_blocks(&item.symbol_name, &mut cc, curr, &b.blocks);
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

fn mk_string(f: FnSymType) -> String {
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


#[test]
fn test_graphviz() {

  use std::io;

  let name = "fubar";

  let mut w = io::stdout();

  let mut writer: DotWriter = DotWriter::from(&mut w);

  writer.set_pretty_print(true);

  let mut digraph = writer.digraph();
  
  digraph
    .node_attributes()
    .set_shape(Shape::Rectangle)
    .set_label(name);

  digraph.node_auto().set_label(name);

  let mut cluster = digraph.cluster();

  cluster
    .set_label("cluck cluck")
    .set_color(Color::Blue);
  cluster.node_named(short_name("cluck_1".to_string()));
  cluster.node_named(block_name("cluck_2".to_string(), 2));
  cluster.edge(short_name("cluck_1".to_string()), block_name("cluck_2".to_string(), 2));

}