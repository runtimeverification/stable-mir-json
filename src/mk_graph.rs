use std::hash::{DefaultHasher, Hash, Hasher};

use crate::printer::{SmirJson};

use dot_writer::{Color, DotWriter, Attributes, Shape, Style};

impl SmirJson<'_> {

  pub fn to_dot_file(self) -> String {
    let mut bytes = Vec::new();

    { 
      let mut writer = DotWriter::from(&mut bytes);

      writer.set_pretty_print(true);

      writer.digraph().set_label(&self.name[..]);
    }

    String::from_utf8(bytes).expect("Error converting dot file")
  }

}
/// consistently naming function clusters
fn short_name(function_name: String) -> String {
  let mut h = DefaultHasher::new();
  function_name.hash(&mut h);
  format!("X{:x}", h.finish())
}

/// consistently naming block nodes in function clusters
fn block_name(function_name: String, id: usize) -> String {
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