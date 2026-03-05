//! Generic MIR graph traversal.
//!
//! This module owns the traversal order and graph semantics.
extern crate stable_mir;
use stable_mir::mir::{Body, Terminator, TerminatorKind};

use crate::printer::SmirJson;
use crate::MonoItemKind;

use crate::mk_graph::context::GraphContext;
use crate::mk_graph::util::{is_unqualified, name_lines, short_name, terminator_targets, hash_body};

/// Represents a call from a block to another function.
///
/// The callee is resolved during traversal and arguments are already
/// rendered as a string. Builders may choose how to visualize this edge.
pub struct CallEdge {
    pub block_idx: usize,
    pub callee_id: String,
    pub callee_name: String,
    pub rendered_args: String,
}

/// A basic block with pre-rendered textual content and structural edges.
///
/// `stmts` and `terminator` are pre-rendered strings produced using
/// `GraphContext`. Builders are free to format or escape them according
/// to their output format.
///
/// `raw_terminator` is provided as an escape hatch for renderers that
/// need to inspect the underlying MIR structure.
pub struct RenderedBlock<'a> {
    pub idx: usize,
    pub stmts: Vec<String>,
    pub terminator: String,
    pub raw_terminator: &'a Terminator,
    pub cfg_edges: Vec<(usize, Option<String>)>,
}

/// A fully analyzed MIR function ready for rendering.
///
/// The traversal layer resolves call targets, renders statements and
/// terminators, and computes the control-flow edges. Builders receive
/// this structure and are responsible only for formatting it into a
/// specific graph representation.
pub struct RenderedFunction<'a> {
    pub id: String,
    pub display_name: String,
    pub is_local: bool,
    pub locals: Vec<(usize, String)>,
    pub blocks: Vec<RenderedBlock<'a>>,
    pub call_edges: Vec<CallEdge>,
}

/// Trait implemented by graph renderers.
///
/// The traversal layer walks the MIR graph and constructs a
/// `RenderedFunction` representation. Implementations of this trait
/// consume those structures and emit format-specific output such as
/// D2, DOT, or other diagram formats.
///
/// The trait intentionally separates graph structure from formatting.
/// Traversal decides *what* the graph contains while the builder
/// decides *how* it is rendered.
pub trait GraphBuilder {
    type Output;

    fn begin_graph(&mut self, name: &str);

    fn alloc_legend(&mut self, lines: &[String]);

    fn type_legend(&mut self, lines: &[String]);

    fn external_function(&mut self, id: &str, name: &str);

    fn render_function(&mut self, func: &RenderedFunction);

    fn static_item(&mut self, id: &str, name: &str);

    fn asm_item(&mut self, id: &str, content: &str);

    fn finish(self) -> Self::Output;
}

/// Traverse the SMIR representation and produce rendered graph data.
///
/// This function performs MIR traversal, resolves call targets, and
/// constructs `RenderedFunction` structures which are then passed to
/// the provided `GraphBuilder`.
pub fn render_graph<B: GraphBuilder>(smir: &SmirJson, mut builder: B) -> B::Output {
    let ctx = GraphContext::from_smir(smir);

    builder.begin_graph(&smir.name);

    builder.alloc_legend(&ctx.allocs_legend_lines());
    builder.type_legend(&ctx.types_legend_lines());

    for item in &smir.items {
        match &item.mono_item_kind {
            MonoItemKind::MonoItemFn { name, body, .. } => {
                let func = render_function(&ctx, name, body.as_ref());
                builder.render_function(&func);
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

/// Emit graph events for a single function body.
/// Traverses blocks, CFG edges, and call edges without renderer-specific logic.
fn render_function<'a>(
    ctx: &GraphContext,
    name: &str,
    body: Option<&'a Body>,
) -> RenderedFunction<'a> {
    let id = match body {
        Some(b) => format!("fn_{}_{}", short_name(name), hash_body(b)),
        None => format!("fn_{}_no_body", short_name(name)),
    };

    let display_name = name_lines(name);
    let is_local = body.is_some();

    let mut blocks = Vec::new();
    let mut call_edges = Vec::new();
    let mut locals = Vec::new();

    if let Some(body) = body {

        for (idx, decl) in body.local_decls() {
            locals.push((idx, ctx.render_type_with_layout(decl.ty)));
        }

        for (idx, block) in body.blocks.iter().enumerate() {

            let stmts = block
                .statements
                .iter()
                .map(|s| ctx.render_stmt(s))
                .collect();

            let terminator = ctx.render_terminator(&block.terminator);

            let cfg_edges = terminator_targets(&block.terminator)
                .into_iter()
                .map(|t| (t, None))
                .collect();

            blocks.push(RenderedBlock {
                idx,
                stmts,
                terminator,
                raw_terminator: &block.terminator,
                cfg_edges,
            });

            if let TerminatorKind::Call { func, args, .. } = &block.terminator.kind {

                if let Some(callee) = ctx.resolve_call_target(func) {

                    if is_unqualified(&callee) {

                        let rendered_args = args
                            .iter()
                            .map(|a| ctx.render_operand(a))
                            .collect::<Vec<_>>()
                            .join(", ");

                        call_edges.push(CallEdge {
                            block_idx: idx,
                            callee_id: short_name(&callee),
                            callee_name: callee,
                            rendered_args,
                        });
                    }
                }
            }
        }
    }

    RenderedFunction {
        id,
        display_name,
        is_local,
        locals,
        blocks,
        call_edges,
    }
}
