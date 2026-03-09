//! D2 diagram format output for MIR graphs.

use crate::compat::stable_mir;
use stable_mir::mir::TerminatorKind;

use crate::printer::SmirJson;
use crate::MonoItemKind;

use crate::mk_graph::context::GraphContext;
use crate::mk_graph::util::{
    escape_d2, is_unqualified, name_lines, short_name, terminator_targets,
};

impl SmirJson {
    /// Convert the MIR to D2 diagram format
    pub fn to_d2_file(self) -> String {
        let ctx = GraphContext::from_smir(&self);
        let mut output = String::new();

        output.push_str("direction: right\n\n");
        render_d2_allocs_legend(&ctx, &mut output);

        for item in self.items {
            match item.mono_item_kind {
                MonoItemKind::MonoItemFn { name, body, .. } => {
                    render_d2_function(&name, body.as_ref(), &ctx, &mut output);
                }
                MonoItemKind::MonoItemGlobalAsm { asm } => {
                    render_d2_asm(&asm, &mut output);
                }
                MonoItemKind::MonoItemStatic { name, .. } => {
                    render_d2_static(&name, &mut output);
                }
            }
        }

        output
    }
}

// =============================================================================
// D2 Rendering Helpers
// =============================================================================

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
    out.push_str(&format!("  label: \"{legend_text}\"\n"));
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
    out.push_str(&format!("{fn_id}: {{\n"));
    out.push_str(&format!("  label: \"{display_name}\"\n"));
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

        let mut label = format!("bb{idx}:");
        for stmt in &stmts {
            label.push_str(&format!("\\n{stmt}"));
        }
        label.push_str(&format!("\\n---\\n{term_str}"));

        out.push_str(&format!("  bb{idx}: \"{label}\"\n"));
    }
}

fn render_d2_block_edges(body: &stable_mir::mir::Body, out: &mut String) {
    for (idx, block) in body.blocks.iter().enumerate() {
        for target in terminator_targets(&block.terminator) {
            out.push_str(&format!("  bb{idx} -> bb{target}\n"));
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
        out.push_str(&format!("{target_id}.style.fill: \"#ffe0e0\"\n"));
        out.push_str(&format!("{fn_id}.bb{idx} -> {target_id}: call\n"));
    }
}

fn render_d2_asm(asm: &str, out: &mut String) {
    let asm_id = short_name(asm);
    let asm_text = escape_d2(&asm.lines().collect::<String>());
    out.push_str(&format!("{asm_id}: \"{asm_text}\" {{\n"));
    out.push_str("  style.fill: \"#ffe0ff\"\n");
    out.push_str("}\n\n");
}

fn render_d2_static(name: &str, out: &mut String) {
    let static_id = short_name(name);
    out.push_str(&format!("{}: \"{}\" {{\n", static_id, escape_d2(name)));
    out.push_str("  style.fill: \"#e0ffe0\"\n");
    out.push_str("}\n\n");
}
