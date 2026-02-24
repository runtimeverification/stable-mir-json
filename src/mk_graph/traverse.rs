//! Generic MIR graph traversal.
//!
//! This module owns the traversal order and graph semantics.

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
