//! D2 diagram format output for MIR graphs.

use crate::compat::stable_mir;
use stable_mir::mir::{TerminatorKind};

use crate::printer::SmirJson;
use crate::MonoItemKind;

use crate::mk_graph::context::GraphContext;
use crate::mk_graph::util::{
    escape_d2, is_unqualified, name_lines, short_name, terminator_targets,
};

use crate::mk_graph::traverse::render_graph;
use crate::mk_graph::traverse::{GraphBuilder, RenderedFunction};

// =============================================================================
// D2 Builder
// =============================================================================

pub struct D2Builder {
    buf: String,
}

impl D2Builder {
    pub fn new() -> Self {
        Self { buf: String::new() }
    }
}

impl GraphBuilder for D2Builder {
    type Output = String;

    fn begin_graph(&mut self, _name: &str) {
        self.buf.push_str("direction: right\n\n");
    }

    fn alloc_legend(&mut self, lines: &[String]) {

        self.buf.push_str("ALLOCS: {\n");
        self.buf.push_str("  style.fill: \"#ffffcc\"\n");
        self.buf.push_str("  style.stroke: \"#999999\"\n");

        let text = lines
            .iter()
            .map(|l| escape_d2(l))
            .collect::<Vec<_>>()
            .join("\\n");

        self.buf.push_str(&format!("  label: \"{}\"\n", text));
        self.buf.push_str("}\n\n");
    }

    fn type_legend(&mut self, _lines: &[String]) {}

    fn external_function(&mut self, id: &str, name: &str) {

        self.buf
            .push_str(&format!("{}: \"{}\"\n", id, escape_d2(name)));
    }

    fn render_function(&mut self, func: &RenderedFunction) {

        self.buf.push_str(&format!("{}: {{\n", func.id));
        self.buf
            .push_str(&format!("  label: \"{}\"\n", escape_d2(&func.display_name)));
        self.buf.push_str("  style.fill: \"#e0e0ff\"\n");

        for block in &func.blocks {

            let mut label = format!("bb{}:", block.idx);

            for stmt in &block.stmts {
                label.push_str(&format!("\\n{}", escape_d2(stmt)));
            }

            label.push_str(&format!("\\n---\\n{}", escape_d2(&block.terminator)));

            self.buf
                .push_str(&format!("  bb{}: \"{}\"\n", block.idx, label));
        }

        for block in &func.blocks {
            for (target, _) in &block.cfg_edges {
                self.buf
                    .push_str(&format!("  bb{} -> bb{}\n", block.idx, target));
            }
        }

        self.buf.push_str("}\n\n");

        for edge in &func.call_edges {

            self.buf.push_str(&format!(
                "{}: \"{}\"\n",
                edge.callee_id,
                escape_d2(&edge.callee_name)
            ));

            self.buf.push_str(&format!(
                "{}.style.fill: \"#ffe0e0\"\n",
                edge.callee_id
            ));

            self.buf.push_str(&format!(
                "{}.bb{} -> {}: call\n",
                func.id,
                edge.block_idx,
                edge.callee_id
            ));
        }
    }

    fn static_item(&mut self, id: &str, name: &str) {

        self.buf
            .push_str(&format!("{}: \"{}\" {{\n", id, escape_d2(name)));
        self.buf.push_str("  style.fill: \"#e0ffe0\"\n");
        self.buf.push_str("}\n\n");
    }

    fn asm_item(&mut self, id: &str, content: &str) {

        let text = escape_d2(&content.lines().collect::<String>());

        self.buf.push_str(&format!("{}: \"{}\" {{\n", id, text));
        self.buf.push_str("  style.fill: \"#ffe0ff\"\n");
        self.buf.push_str("}\n\n");
    }

    fn finish(self) -> String {
        self.buf
    }
}

// =============================================================================
// Public entry point
// =============================================================================

impl SmirJson<'_> {
    /// Convert the MIR to D2 diagram format
    pub fn to_d2_file(self) -> String {
        let ctx = GraphContext::from_smir(&self);
        let mut output = String::new();

        output.push_str("direction: right\n\n");
        render_d2_allocs_legend(&ctx, &mut output);

        render_d2_items(&self.items, &ctx, &mut output);

        output
    }

    /// Convert the MIR to D2 using GraphBuilder traversal (experimental)
    pub fn to_d2_file_new(&self) -> String {
        render_graph(self, D2Builder::new())
    }
}

// =============================================================================
// D2 Rendering Helpers
// =============================================================================

fn render_d2_items(items: &[crate::printer::Item], ctx: &GraphContext, out: &mut String) {
    for item in items {
        match &item.mono_item_kind {
            MonoItemKind::MonoItemFn { name, body, .. } => {
                render_d2_function(name, body.as_ref(), ctx, out);
            }
            MonoItemKind::MonoItemGlobalAsm { asm } => {
                render_d2_asm(asm, out);
            }
            MonoItemKind::MonoItemStatic { name, .. } => {
                render_d2_static(name, out);
            }
        }
    }
}

fn render_d2_allocs_legend(ctx: &GraphContext, out: &mut String) {
    let legend_lines = ctx.allocs_legend_lines();

    out.push_str("ALLOCS: {\n");
    out.push_str("  style.fill: \"#ffffcc\"\n");
    out.push_str("  style.stroke: \"#999999\"\n");
    let legend_text = legend_lines
        .iter()
        .map(|s| escape_d2(s))
        .collect::<Vec<_>>()
        .join("\\n");
    out.push_str(&format!("  label: \"{}\"\n", legend_text));
    out.push_str("}\n\n");
}

fn render_d2_function(
    name: &str,
    body: Option<&stable_mir::mir::Body>,
    ctx: &GraphContext,
    out: &mut String,
) {
    let fn_id = short_name(name);
    let display_name = escape_d2(&name_lines(name));

    // Function container
    out.push_str(&format!("{}: {{\n", fn_id));
    out.push_str(&format!("  label: \"{}\"\n", display_name));
    out.push_str("  style.fill: \"#e0e0ff\"\n");

    if let Some(body) = body {
        render_d2_blocks(body, ctx, out);
        render_d2_block_edges(body, out);
    }

    out.push_str("}\n\n");

    // Call edges (must be outside the container)
    if let Some(body) = body {
        render_d2_call_edges(&fn_id, body, ctx, out);
    }
}

fn render_d2_blocks(body: &stable_mir::mir::Body, ctx: &GraphContext, out: &mut String) {
    for (idx, block) in body.blocks.iter().enumerate() {
        let stmts: Vec<String> = block
            .statements
            .iter()
            .map(|s| escape_d2(&ctx.render_stmt(s)))
            .collect();
        let term_str = escape_d2(&ctx.render_terminator(&block.terminator));

        let mut label = format!("bb{}:", idx);
        for stmt in &stmts {
            label.push_str(&format!("\\n{}", stmt));
        }
        label.push_str(&format!("\\n---\\n{}", term_str));

        out.push_str(&format!("  bb{}: \"{}\"\n", idx, label));
    }
}

fn render_d2_block_edges(body: &stable_mir::mir::Body, out: &mut String) {
    for (idx, block) in body.blocks.iter().enumerate() {
        for target in terminator_targets(&block.terminator) {
            out.push_str(&format!("  bb{} -> bb{}\n", idx, target));
        }
    }
}

fn render_d2_call_edges(
    fn_id: &str,
    body: &stable_mir::mir::Body,
    ctx: &GraphContext,
    out: &mut String,
) {
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

        let target_id = short_name(&callee_name);
        out.push_str(&format!("{}: \"{}\"\n", target_id, escape_d2(&callee_name)));
        out.push_str(&format!("{}.style.fill: \"#ffe0e0\"\n", target_id));
        out.push_str(&format!("{}.bb{} -> {}: call\n", fn_id, idx, target_id));
    }
}

fn render_d2_asm(asm: &str, out: &mut String) {
    let asm_id = short_name(asm);
    let asm_text = escape_d2(&asm.lines().collect::<String>());
    out.push_str(&format!("{}: \"{}\" {{\n", asm_id, asm_text));
    out.push_str("  style.fill: \"#ffe0ff\"\n");
    out.push_str("}\n\n");
}

fn render_d2_static(name: &str, out: &mut String) {
    let static_id = short_name(name);
    out.push_str(&format!("{}: \"{}\" {{\n", static_id, escape_d2(name)));
    out.push_str("  style.fill: \"#e0ffe0\"\n");
    out.push_str("}\n\n");
}
