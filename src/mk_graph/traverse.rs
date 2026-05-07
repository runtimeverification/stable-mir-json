//! Generic MIR graph traversal.
//!
//! This module owns the traversal order and graph semantics.
extern crate stable_mir;
use stable_mir::mir::{Body, TerminatorKind, UnwindAction};

use crate::printer::SmirJson;
use crate::MonoItemKind;

use crate::mk_graph::context::GraphContext;
use crate::mk_graph::util::{hash_body, is_unqualified, name_lines, short_name, GraphLabelString};

use std::collections::{HashMap, HashSet};

/// Represents a call from a block to another function.
///
/// The callee is resolved during traversal and arguments are already
/// rendered as a string. Builders may choose how to visualize this edge.
pub struct CallEdge {
    pub block_idx: usize,
    pub callee_name: String,
    pub rendered_args: String,
}

/// A basic block with pre-rendered textual content and structural edges.
///
/// `stmts` and `terminator` are pre-rendered strings produced using
/// `GraphContext`. Builders are free to format or escape them according
/// to their output format.
pub struct RenderedBlock {
    pub idx: usize,
    pub stmts: Vec<String>,
    pub terminator: String,
    pub cfg_edges: Vec<(usize, Option<String>)>,
}

/// A fully analyzed MIR function ready for rendering.
///
/// The traversal layer resolves call targets, renders statements and
/// terminators, and computes the control-flow edges. Builders receive
/// this structure and are responsible only for formatting it into a
/// specific graph representation.
pub struct RenderedFunction {
    pub id: String,
    pub symbol_name: String,
    pub is_unqualified: bool,
    pub display_name: String,
    pub locals: Vec<(usize, String)>,
    pub blocks: Vec<RenderedBlock>,
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

    fn external_function(&mut self, name: &str);

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

    // Full symbol names of all defined mono items. Used to suppress
    // external_function calls for callees that have a defined body.
    let defined_symbol_names: HashSet<String> =
        smir.items.iter().map(|i| i.symbol_name.clone()).collect();

    // Accumulates all reachable callees: full symbol name -> full symbol name.
    let mut called: HashMap<String, String> = HashMap::new();

    for item in &smir.items {
        match &item.mono_item_kind {
            MonoItemKind::MonoItemFn { name, body, .. } => {
                let func = render_function(&ctx, name, &item.symbol_name, body.as_ref());

                for edge in &func.call_edges {
                    called
                        .entry(edge.callee_name.clone())
                        .or_insert(edge.callee_name.clone());
                }

                builder.render_function(&func);
            }
            MonoItemKind::MonoItemStatic { name, .. } => {
                builder.static_item(&short_name(name), name);
            }
            MonoItemKind::MonoItemGlobalAsm { asm } => {
                builder.asm_item(&short_name(asm), asm);
            }
        }
    }

    // Emit external nodes only for callees with no defined body.
    for (name, _) in called {
        if !defined_symbol_names.contains(&name) {
            builder.external_function(&name);
        }
    }

    builder.finish()
}

/// Emit graph events for a single function body.
/// Traverses blocks, CFG edges, and call edges without renderer-specific logic.
fn render_function(
    ctx: &GraphContext,
    name: &str,
    symbol_name: &str,
    body: Option<&Body>,
) -> RenderedFunction {
    let id = match body {
        Some(b) => format!("fn_{}_{}", short_name(name), hash_body(b)),
        None => format!("fn_{}_no_body", short_name(name)),
    };

    let display_name = name_lines(name);
    let unqualified = is_unqualified(name);

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

            let mut cfg_edges = Vec::new();

            match &block.terminator.kind {
                TerminatorKind::Goto { target } => {
                    cfg_edges.push((*target, None));
                }

                TerminatorKind::SwitchInt { targets, .. } => {
                    for (value, target) in targets.branches() {
                        cfg_edges.push((target, Some(value.to_string())));
                    }
                    cfg_edges.push((targets.otherwise(), Some("other".into())));
                }

                TerminatorKind::Return
                | TerminatorKind::Abort
                | TerminatorKind::Resume
                | TerminatorKind::Unreachable => {}

                TerminatorKind::Drop { target, unwind, .. } => {
                    cfg_edges.push((*target, None));
                    if let UnwindAction::Cleanup(t) = unwind {
                        cfg_edges.push((*t, Some("cleanup".into())));
                    }
                }

                TerminatorKind::Call {
                    destination,
                    target,
                    unwind,
                    ..
                } => {
                    if let Some(t) = target {
                        cfg_edges.push((*t, Some(destination.label())));
                    }
                    if let UnwindAction::Cleanup(t) = unwind {
                        cfg_edges.push((*t, Some("cleanup".into())));
                    }
                }

                TerminatorKind::Assert { target, unwind, .. } => {
                    cfg_edges.push((*target, None));
                    if let UnwindAction::Cleanup(t) = unwind {
                        cfg_edges.push((*t, Some("cleanup".into())));
                    }
                }

                TerminatorKind::InlineAsm {
                    destination,
                    unwind,
                    ..
                } => {
                    if let Some(t) = destination {
                        cfg_edges.push((*t, None));
                    }
                    if let UnwindAction::Cleanup(t) = unwind {
                        cfg_edges.push((*t, Some("cleanup".into())));
                    }
                }
            }

            blocks.push(RenderedBlock {
                idx,
                stmts,
                terminator,
                cfg_edges,
            });

            // Collect call edges for all resolvable call targets.
            // No is_unqualified guard here
            if let TerminatorKind::Call { func, args, .. } = &block.terminator.kind {
                if let Some(callee) = ctx.resolve_call_target(func) {
                    let rendered_args = args
                        .iter()
                        .map(|a| ctx.render_operand(a))
                        .collect::<Vec<_>>()
                        .join(", ");

                    call_edges.push(CallEdge {
                        block_idx: idx,
                        callee_name: callee,
                        rendered_args,
                    });
                }
            }
        }
    }

    RenderedFunction {
        id,
        symbol_name: symbol_name.to_string(),
        is_unqualified: unqualified,
        display_name,
        locals,
        blocks,
        call_edges,
    }
}
