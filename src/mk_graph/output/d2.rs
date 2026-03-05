//! D2 diagram format output for MIR graphs.

use crate::printer::SmirJson;

use crate::mk_graph::util::escape_d2;

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

impl Default for D2Builder {
    fn default() -> Self {
        Self::new()
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

            self.buf
                .push_str(&format!("{}.style.fill: \"#ffe0e0\"\n", edge.callee_id));

            self.buf.push_str(&format!(
                "{}.bb{} -> {}: call\n",
                func.id, edge.block_idx, edge.callee_id
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

impl SmirJson {
    /// Convert the MIR to D2 using GraphBuilder traversal
    pub fn to_d2_file(&self) -> String {
        render_graph(self, D2Builder::new())
    }
}
