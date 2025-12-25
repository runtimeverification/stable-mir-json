//! DOT (Graphviz) format output for MIR graphs.

use std::collections::HashSet;

use dot_writer::{Attributes, Color, DotWriter, Scope, Shape, Style};

extern crate stable_mir;
use stable_mir::mir::{BasicBlock, ConstOperand, Operand, TerminatorKind, UnwindAction};

use crate::printer::SmirJson;
use crate::MonoItemKind;

use crate::mk_graph::context::GraphContext;
use crate::mk_graph::util::{block_name, is_unqualified, name_lines, short_name, GraphLabelString};

impl SmirJson<'_> {
    /// Convert the MIR to DOT (Graphviz) format
    pub fn to_dot_file(self) -> String {
        let mut bytes = Vec::new();

        // Build context BEFORE consuming self
        let ctx = GraphContext::from_smir(&self);

        {
            let mut writer = DotWriter::from(&mut bytes);

            writer.set_pretty_print(true);

            let mut graph = writer.digraph();
            graph.set_label(&self.name[..]);
            graph.node_attributes().set_shape(Shape::Rectangle);

            let item_names: HashSet<String> =
                self.items.iter().map(|i| i.symbol_name.clone()).collect();

            // Add allocs legend node if there are any allocs
            if !ctx.allocs.by_id.is_empty() {
                let mut alloc_node = graph.node_auto();
                let mut lines = ctx.allocs_legend_lines();
                lines.push("".to_string());
                alloc_node.set_label(&lines.join("\\l"));
                alloc_node.set_style(Style::Filled);
                alloc_node.set("color", "lightyellow", false);
            }

            // first create all nodes for functions not in the items list
            for f in ctx.functions.values() {
                if !item_names.contains(f) {
                    graph
                        .node_named(block_name(f, 0))
                        .set_label(&name_lines(f))
                        .set_color(Color::Red);
                }
            }

            for item in self.items {
                match item.mono_item_kind {
                    MonoItemKind::MonoItemFn { name, body, id: _ } => {
                        let mut c = graph.cluster();
                        c.set_label(&name_lines(&name));
                        c.set_style(Style::Filled);
                        if is_unqualified(&name) {
                            c.set_color(Color::PaleGreen);
                        } else {
                            c.set_color(Color::LightGrey);
                        }

                        // Set out the type information of the locals
                        let mut local_node = c.node_auto();
                        let mut vector: Vec<String> = vec![];
                        vector.push(String::from("LOCALS"));
                        for (index, decl) in body.clone().unwrap().local_decls() {
                            vector.push(format!("{index} = {}", decl.ty));
                        }
                        vector.push("".to_string());
                        local_node.set_label(vector.join("\\l").to_string().as_str());
                        local_node.set_style(Style::Filled);
                        local_node.set("color", "palegreen3", false);
                        drop(local_node);

                        // Cannot define local functions that capture env. variables. Instead we define _closures_.
                        let process_block =
                            |cluster: &mut Scope<'_, '_>, node_id: usize, b: &BasicBlock| {
                                let name = &item.symbol_name;
                                let this_block = block_name(name, node_id);

                                let mut label_strs: Vec<String> =
                                    b.statements.iter().map(|s| ctx.render_stmt(s)).collect();

                                use TerminatorKind::*;
                                match &b.terminator.kind {
                                    Goto { target } => {
                                        label_strs.push("Goto".to_string());
                                        cluster.edge(&this_block, block_name(name, *target));
                                    }
                                    SwitchInt { discr, targets } => {
                                        label_strs.push(format!(
                                            "SwitchInt {}",
                                            ctx.render_operand(discr)
                                        ));
                                        for (d, t) in targets.clone().branches() {
                                            cluster
                                                .edge(&this_block, block_name(name, t))
                                                .attributes()
                                                .set_label(&format!("{d}"));
                                        }
                                        cluster
                                            .edge(
                                                &this_block,
                                                block_name(name, targets.otherwise()),
                                            )
                                            .attributes()
                                            .set_label("other");
                                    }
                                    Resume {} => {
                                        label_strs.push("Resume".to_string());
                                    }
                                    Abort {} => {
                                        label_strs.push("Abort".to_string());
                                    }
                                    Return {} => {
                                        label_strs.push("Return".to_string());
                                    }
                                    Unreachable {} => {
                                        label_strs.push("Unreachable".to_string());
                                    }
                                    TerminatorKind::Drop {
                                        place,
                                        target,
                                        unwind,
                                    } => {
                                        label_strs.push(format!("Drop {}", place.label()));
                                        if let UnwindAction::Cleanup(t) = unwind {
                                            cluster
                                                .edge(&this_block, block_name(name, *t))
                                                .attributes()
                                                .set_label("Cleanup");
                                        }
                                        cluster.edge(&this_block, block_name(name, *target));
                                    }
                                    Call {
                                        func: _,
                                        args: _,
                                        destination,
                                        target,
                                        unwind,
                                    } => {
                                        label_strs.push("Call".to_string());
                                        if let UnwindAction::Cleanup(t) = unwind {
                                            cluster
                                                .edge(&this_block, block_name(name, *t))
                                                .attributes()
                                                .set_label("Cleanup");
                                        }
                                        if let Some(t) = target {
                                            let dest = destination.label();
                                            cluster
                                                .edge(&this_block, block_name(name, *t))
                                                .attributes()
                                                .set_label(&dest);
                                        }

                                        // The call edge has to be drawn outside the cluster, outside this function (cluster borrows &mut graph)!
                                        // Code for that is therefore separated into its own second function below.
                                    }
                                    Assert {
                                        cond,
                                        expected,
                                        msg: _,
                                        target,
                                        unwind,
                                    } => {
                                        label_strs.push(format!(
                                            "Assert {} == {}",
                                            ctx.render_operand(cond),
                                            expected
                                        ));
                                        if let UnwindAction::Cleanup(t) = unwind {
                                            cluster
                                                .edge(&this_block, block_name(name, *t))
                                                .attributes()
                                                .set_label("Cleanup");
                                        }
                                        cluster.edge(&this_block, block_name(name, *target));
                                    }
                                    InlineAsm {
                                        destination,
                                        unwind,
                                        ..
                                    } => {
                                        label_strs.push("Inline ASM".to_string());
                                        if let Some(t) = destination {
                                            cluster.edge(&this_block, block_name(name, *t));
                                        }
                                        if let UnwindAction::Cleanup(t) = unwind {
                                            cluster
                                                .edge(&this_block, block_name(name, *t))
                                                .attributes()
                                                .set_label("Cleanup");
                                        }
                                    }
                                }
                                let mut n = cluster.node_named(&this_block);
                                label_strs.push("".to_string());
                                n.set_label(&label_strs.join("\\l"));
                            };

                        let process_blocks =
                            |cluster: &mut Scope<'_, '_>, offset, blocks: &Vec<BasicBlock>| {
                                let mut n: usize = offset;
                                for b in blocks {
                                    process_block(cluster, n, b);
                                    n += 1;
                                }
                            };

                        if let Some(body) = &body {
                            process_blocks(&mut c, 0, &body.blocks);
                        } else {
                            c.node_auto().set_label("<empty body>");
                        }

                        drop(c); // so we can borrow graph again

                        // call edges have to be added _outside_ the cluster of blocks for one function
                        // because they go between different clusters. Due to a scope/borrow issue, we have
                        // to make a 2nd pass over the bodies of the item.
                        let add_call_edges =
                            |graph: &mut Scope<'_, '_>, offset: usize, bs: &Vec<BasicBlock>| {
                                for (i, b) in bs.iter().enumerate() {
                                    let this_block = block_name(&item.symbol_name, offset + i);

                                    match &b.terminator.kind {
                                        TerminatorKind::Call { func, args, .. } => {
                                            let e = match func {
                                                Operand::Constant(ConstOperand {
                                                    const_, ..
                                                }) => {
                                                    if let Some(callee) =
                                                        ctx.functions.get(&const_.ty())
                                                    {
                                                        // callee node/body will be added when its body is added, missing ones added before
                                                        graph.edge(
                                                            &this_block,
                                                            block_name(callee, 0),
                                                        )
                                                    } else {
                                                        let unknown = format!("{}", const_.ty());
                                                        // pathological case, could panic! instead.
                                                        // all unknown callees will be collapsed into one `unknown` node
                                                        graph.edge(&this_block, unknown)
                                                    }
                                                }
                                                Operand::Copy(place) => graph.edge(
                                                    &this_block,
                                                    format!("{}: {}", &this_block, place.label()),
                                                ),
                                                Operand::Move(place) => graph.edge(
                                                    &this_block,
                                                    format!("{}: {}", &this_block, place.label()),
                                                ),
                                            };
                                            let arg_str = args
                                                .iter()
                                                .map(|op| ctx.render_operand(op))
                                                .collect::<Vec<String>>()
                                                .join(",");
                                            e.attributes().set_label(&arg_str);
                                        }
                                        _other => {
                                            // nothing to do
                                        }
                                    }
                                }
                            };

                        if let Some(ref body) = body {
                            add_call_edges(&mut graph, 0, &body.blocks);
                        }
                    }
                    MonoItemKind::MonoItemGlobalAsm { asm } => {
                        let mut n = graph.node_named(short_name(&asm));
                        n.set_label(&asm.lines().collect::<String>()[..]);
                    }
                    MonoItemKind::MonoItemStatic {
                        name,
                        id: _,
                        allocation: _,
                    } => {
                        let mut n = graph.node_named(short_name(&name));
                        n.set_label(&name[..]);
                    }
                }
            }
        }

        String::from_utf8(bytes).expect("Error converting dot file")
    }
}
