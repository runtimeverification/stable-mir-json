//! Generic MIR graph traversal.
//!
//! This module owns the traversal order and graph semantics.
extern crate stable_mir;
use stable_mir::mir::TerminatorKind;

use crate::printer::SmirJson;
use crate::MonoItemKind;

use crate::mk_graph::context::GraphContext;
use crate::mk_graph::util::{
    is_unqualified, name_lines, short_name, terminator_targets,
};

/// Format agnostic graph sink.
/// Implemented by all renderers.
pub trait GraphBuilder {
    type Output;

    fn begin_graph(&mut self, name: &str);

    fn alloc_legend(&mut self, lines: &[String]);

    fn type_legend(&mut self, lines: &[String]);

    fn begin_function(&mut self, id: &str, label: &str, is_local: bool);

    fn block(&mut self, fn_id: &str, idx: usize, stmts: &[String], terminator: &str);

    fn block_edge(&mut self, fn_id: &str, from: usize, to: usize, label: Option<&str>);

    fn call_edge(&mut self, fn_id: &str, block: usize, callee_id: &str, callee_name: &str);

    fn end_function(&mut self, id: &str);

    fn static_item(&mut self, id: &str, name: &str);

    fn asm_item(&mut self, id: &str, content: &str);

    fn finish(self) -> Self::Output;
}

pub fn render_graph<B: GraphBuilder>(
    smir: &SmirJson,
    mut builder: B,
) -> B::Output {
    let ctx = GraphContext::from_smir(smir);

    builder.begin_graph(&smir.name);

    builder.alloc_legend(&ctx.allocs_legend_lines());
    builder.type_legend(&ctx.types_legend_lines());

    for item in &smir.items {
        match &item.mono_item_kind {
            MonoItemKind::MonoItemFn { name, body, .. } => {
                render_function(&ctx, &mut builder, name, body.as_ref());
            }
            MonoItemKind::MonoItemStatic { name, .. } => {
                let id = short_name(name);
                builder.static_item(&id, name);
            }
            MonoItemKind::MonoItemGlobalAsm { asm } => {
                let id = short_name(asm);
                builder.asm_item(&id, asm);
            }
        }
    }

    builder.finish()
}

fn render_function<B: GraphBuilder>(
    ctx: &GraphContext,
    builder: &mut B,
    name: &str,
    body: Option<&stable_mir::mir::Body>,
) {
    let fn_id = short_name(name);
    let label = name_lines(name);
    let is_local = true;

    builder.begin_function(&fn_id, &label, is_local);

    if let Some(body) = body {
        // blocks
        for (idx, block) in body.blocks.iter().enumerate() {
            let stmts = block
                .statements
                .iter()
                .map(|s| ctx.render_stmt(s))
                .collect::<Vec<_>>();

            let term = ctx.render_terminator(&block.terminator);

            builder.block(&fn_id, idx, &stmts, &term);
        }

        // CFG edges
        for (idx, block) in body.blocks.iter().enumerate() {
            for target in terminator_targets(&block.terminator) {
                builder.block_edge(&fn_id, idx, target, None);
            }
        }
    }

    builder.end_function(&fn_id);

    // Call edges (outside container)
    if let Some(body) = body {
        for (idx, block) in body.blocks.iter().enumerate() {
            let TerminatorKind::Call { func, .. } = &block.terminator.kind else {
                continue;
            };

            let Some(callee_name) = ctx.resolve_call_target(func) else {
                continue;
            };

            if !is_unqualified(&callee_name) {
                continue;
            }

            let callee_id = short_name(&callee_name);
            builder.call_edge(&fn_id, idx, &callee_id, &callee_name);
        }
    }
}
