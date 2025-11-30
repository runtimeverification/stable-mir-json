use std::{
    collections::{HashMap, HashSet},
    fs::File,
    hash::{DefaultHasher, Hash, Hasher},
    io::{self, Write},
};

use dot_writer::{Attributes, Color, DotWriter, Scope, Shape, Style};

extern crate rustc_middle;
use rustc_middle::ty::TyCtxt;

extern crate stable_mir;
use rustc_session::config::{OutFileName, OutputType};

extern crate rustc_session;
use stable_mir::mir::alloc::GlobalAlloc;
use stable_mir::ty::{ConstantKind, IndexedVal, MirConst, Ty};
use stable_mir::CrateDef;
use stable_mir::{
    mir::{
        AggregateKind, BasicBlock, BorrowKind, ConstOperand, Mutability, NonDivergingIntrinsic,
        NullOp, Operand, Place, ProjectionElem, Rvalue, Statement, StatementKind, Terminator,
        TerminatorKind, UnwindAction,
    },
    ty::RigidTy,
};

use crate::{
    printer::{collect_smir, AllocInfo, FnSymType, SmirJson, TypeMetadata},
    MonoItemKind,
};

// =============================================================================
// Graph Index Structures
// =============================================================================

/// Index for looking up allocation information by AllocId
pub struct AllocIndex {
    pub by_id: HashMap<u64, AllocEntry>,
}

/// Processed allocation entry with human-readable description
pub struct AllocEntry {
    pub alloc_id: u64,
    pub ty: Ty,
    pub kind: AllocKind,
    pub description: String,
}

/// Simplified allocation kind for display
pub enum AllocKind {
    Memory { bytes_len: usize, is_str: bool },
    Static { name: String },
    VTable { ty_desc: String },
    Function { name: String },
}

/// Index for looking up type information
pub struct TypeIndex {
    by_id: HashMap<u64, String>,
}

/// Context for rendering graph labels with access to indices
pub struct GraphContext {
    pub allocs: AllocIndex,
    pub types: TypeIndex,
    pub functions: HashMap<Ty, String>,
}

// =============================================================================
// Index Implementation
// =============================================================================

impl AllocIndex {
    pub fn new() -> Self {
        Self {
            by_id: HashMap::new(),
        }
    }

    pub fn from_alloc_infos(allocs: &[AllocInfo], type_index: &TypeIndex) -> Self {
        let mut index = Self::new();
        for info in allocs {
            let entry = AllocEntry::from_alloc_info(info, type_index);
            index.by_id.insert(entry.alloc_id, entry);
        }
        index
    }

    pub fn get(&self, id: u64) -> Option<&AllocEntry> {
        self.by_id.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &AllocEntry> {
        self.by_id.values()
    }

    /// Describe an alloc by its ID for use in labels
    pub fn describe(&self, id: u64) -> String {
        match self.get(id) {
            Some(entry) => entry.short_description(),
            None => format!("alloc{}", id),
        }
    }
}

impl AllocEntry {
    pub fn from_alloc_info(info: &AllocInfo, type_index: &TypeIndex) -> Self {
        let alloc_id = info.alloc_id().to_index() as u64;
        let ty = info.ty();
        let ty_name = type_index.get_name(ty);

        let (kind, description) = match info.global_alloc() {
            GlobalAlloc::Memory(alloc) => {
                let bytes = &alloc.bytes;
                let is_str = ty_name.contains("str") || ty_name.contains("&str");

                // Convert Option<u8> bytes to actual bytes for display
                let concrete_bytes: Vec<u8> = bytes.iter().filter_map(|&b| b).collect();

                let desc = if is_str && concrete_bytes.iter().all(|b| b.is_ascii()) {
                    let s: String = concrete_bytes
                        .iter()
                        .take(20)
                        .map(|&b| b as char)
                        .collect::<String>()
                        .escape_default()
                        .to_string();
                    if concrete_bytes.len() > 20 {
                        format!("\"{}...\" ({} bytes)", s, concrete_bytes.len())
                    } else {
                        format!("\"{}\"", s)
                    }
                } else if concrete_bytes.len() <= 8 && !concrete_bytes.is_empty() {
                    format!("{} = {}", ty_name, bytes_to_u64_le(&concrete_bytes))
                } else {
                    format!("{} ({} bytes)", ty_name, bytes.len())
                };

                (
                    AllocKind::Memory {
                        bytes_len: bytes.len(),
                        is_str,
                    },
                    desc,
                )
            }
            GlobalAlloc::Static(def) => {
                let name = def.name();
                (
                    AllocKind::Static { name: name.clone() },
                    format!("static {}", name),
                )
            }
            GlobalAlloc::VTable(vty, _trait_ref) => {
                let desc = format!("{}", vty);
                (
                    AllocKind::VTable {
                        ty_desc: desc.clone(),
                    },
                    format!("vtable<{}>", desc),
                )
            }
            GlobalAlloc::Function(instance) => {
                let name = instance.name();
                (
                    AllocKind::Function { name: name.clone() },
                    format!("fn {}", name),
                )
            }
        };

        Self {
            alloc_id,
            ty,
            kind,
            description,
        }
    }

    pub fn short_description(&self) -> String {
        format!("alloc{}: {}", self.alloc_id, self.description)
    }
}

impl TypeIndex {
    pub fn new() -> Self {
        Self {
            by_id: HashMap::new(),
        }
    }

    pub fn from_types(types: &[(Ty, TypeMetadata)]) -> Self {
        let mut index = Self::new();
        for (ty, metadata) in types {
            let name = Self::type_name_from_metadata(metadata, *ty);
            index.by_id.insert(ty.to_index() as u64, name);
        }
        index
    }

    fn type_name_from_metadata(metadata: &TypeMetadata, ty: Ty) -> String {
        match metadata {
            TypeMetadata::PrimitiveType(rigid) => format!("{:?}", rigid),
            TypeMetadata::EnumType { name, .. } => name.clone(),
            TypeMetadata::StructType { name, .. } => name.clone(),
            TypeMetadata::UnionType { name, .. } => name.clone(),
            TypeMetadata::ArrayType { .. } => format!("{}", ty),
            TypeMetadata::PtrType { .. } => format!("{}", ty),
            TypeMetadata::RefType { .. } => format!("{}", ty),
            TypeMetadata::TupleType { .. } => format!("{}", ty),
            TypeMetadata::FunType(name) => name.clone(),
            TypeMetadata::VoidType => "()".to_string(),
        }
    }

    pub fn get_name(&self, ty: Ty) -> String {
        self.by_id
            .get(&(ty.to_index() as u64))
            .cloned()
            .unwrap_or_else(|| format!("{}", ty))
    }
}

impl GraphContext {
    pub fn from_smir(smir: &SmirJson) -> Self {
        let types = TypeIndex::from_types(&smir.types);
        let allocs = AllocIndex::from_alloc_infos(&smir.allocs, &types);
        let functions: HashMap<Ty, String> = smir
            .functions
            .iter()
            .map(|(k, v)| (k.0, function_string(v.clone())))
            .collect();

        Self {
            allocs,
            types,
            functions,
        }
    }

    /// Render a constant operand with alloc information
    pub fn render_const(&self, const_: &MirConst) -> String {
        let ty = const_.ty();
        let ty_name = self.types.get_name(ty);

        match const_.kind() {
            ConstantKind::Allocated(alloc) => {
                // Check if this constant references any allocs via provenance
                if !alloc.provenance.ptrs.is_empty() {
                    let alloc_refs: Vec<String> = alloc
                        .provenance
                        .ptrs
                        .iter()
                        .map(|(_offset, prov)| self.allocs.describe(prov.0.to_index() as u64))
                        .collect();
                    format!("const [{}]", alloc_refs.join(", "))
                } else {
                    // Inline constant - try to show value
                    let bytes = &alloc.bytes;
                    // Convert Option<u8> to concrete bytes
                    let concrete_bytes: Vec<u8> = bytes.iter().filter_map(|&b| b).collect();
                    if concrete_bytes.len() <= 8 && !concrete_bytes.is_empty() {
                        format!("const {}_{}", bytes_to_u64_le(&concrete_bytes), ty_name)
                    } else {
                        format!("const {}", ty_name)
                    }
                }
            }
            ConstantKind::ZeroSized => {
                // Function pointers, unit type, etc.
                if ty.kind().is_fn() {
                    if let Some(name) = self.functions.get(&ty) {
                        format!("const fn {}", short_fn_name(name))
                    } else {
                        format!("const {}", ty_name)
                    }
                } else {
                    format!("const {}", ty_name)
                }
            }
            ConstantKind::Ty(_) => format!("const {}", ty_name),
            ConstantKind::Unevaluated(_) => format!("const unevaluated {}", ty_name),
            ConstantKind::Param(_) => format!("const param {}", ty_name),
        }
    }

    /// Render an operand with context
    pub fn render_operand(&self, op: &Operand) -> String {
        match op {
            Operand::Constant(ConstOperand { const_, .. }) => self.render_const(const_),
            Operand::Copy(place) => format!("cp({})", place.label()),
            Operand::Move(place) => format!("mv({})", place.label()),
        }
    }

    /// Generate the allocs legend as lines for display
    pub fn allocs_legend_lines(&self) -> Vec<String> {
        let mut lines = vec!["ALLOCS".to_string()];
        let mut entries: Vec<_> = self.allocs.iter().collect();
        entries.sort_by_key(|e| e.alloc_id);
        for entry in entries {
            lines.push(entry.short_description());
        }
        lines
    }

    /// Resolve a call target to a function name if it's a constant function pointer
    pub fn resolve_call_target(&self, func: &Operand) -> Option<String> {
        match func {
            Operand::Constant(ConstOperand { const_, .. }) => {
                let ty = const_.ty();
                if ty.kind().is_fn() {
                    self.functions.get(&ty).cloned()
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Shorten a function name for display
fn short_fn_name(name: &str) -> String {
    // Take last segment after ::
    name.rsplit("::").next().unwrap_or(name).to_string()
}

// entry point to write the dot file
pub fn emit_dotfile(tcx: TyCtxt<'_>) {
    let smir_dot = collect_smir(tcx).to_dot_file();

    match tcx.output_filenames(()).path(OutputType::Mir) {
        OutFileName::Stdout => {
            write!(io::stdout(), "{}", smir_dot).expect("Failed to write smir.dot");
        }
        OutFileName::Real(path) => {
            let out_path = path.with_extension("smir.dot");
            let mut b = io::BufWriter::new(File::create(&out_path).unwrap_or_else(|e| {
                panic!("Failed to create {}: {}", out_path.display(), e)
            }));
            write!(b, "{}", smir_dot).expect("Failed to write smir.dot");
        }
    }
}

// entry point to write the d2 file
pub fn emit_d2file(tcx: TyCtxt<'_>) {
    let smir_d2 = collect_smir(tcx).to_d2_file();

    match tcx.output_filenames(()).path(OutputType::Mir) {
        OutFileName::Stdout => {
            write!(io::stdout(), "{}", smir_d2).expect("Failed to write smir.d2");
        }
        OutFileName::Real(path) => {
            let out_path = path.with_extension("smir.d2");
            let mut b = io::BufWriter::new(File::create(&out_path).unwrap_or_else(|e| {
                panic!("Failed to create {}: {}", out_path.display(), e)
            }));
            write!(b, "{}", smir_d2).expect("Failed to write smir.d2");
        }
    }
}

impl SmirJson<'_> {
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
                                    b.statements.iter().map(|s| render_stmt_ctx(s, &ctx)).collect();
                                // TODO: render statements and terminator as text label (with line breaks)
                                // switch on terminator kind, add inner and out-edges according to terminator
                                use TerminatorKind::*;
                                match &b.terminator.kind {
                                    Goto { target } => {
                                        label_strs.push("Goto".to_string());
                                        cluster.edge(&this_block, block_name(name, *target));
                                    }
                                    SwitchInt { discr, targets } => {
                                        label_strs.push(format!("SwitchInt {}", ctx.render_operand(discr)));
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
                                                    if let Some(callee) = ctx.functions.get(&const_.ty())
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

    /// Convert to D2 diagram format
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
    if legend_lines.is_empty() {
        return;
    }

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
            .map(|s| escape_d2(&render_stmt_ctx(s, ctx)))
            .collect();
        let term_str = escape_d2(&render_terminator_ctx(&block.terminator, ctx));

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

/// Escape special characters for D2 string labels
fn escape_d2(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
}

/// Convert byte slice to u64, little-endian (least significant byte first)
fn bytes_to_u64_le(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .enumerate()
        .fold(0u64, |acc, (i, &b)| acc | ((b as u64) << (i * 8)))
}

fn is_unqualified(name: &str) -> bool {
    !name.contains("::")
}

fn function_string(f: FnSymType) -> String {
    match f {
        FnSymType::NormalSym(name) => name,
        FnSymType::NoOpSym(name) => format!("NoOp: {name}"),
        FnSymType::IntrinsicSym(name) => format!("Intr: {name}"),
    }
}

fn name_lines(name: &str) -> String {
    name.split_inclusive(" ")
        .flat_map(|s| s.as_bytes().chunks(25))
        .map(|bs| core::str::from_utf8(bs).unwrap().to_string())
        .collect::<Vec<String>>()
        .join("\\n")
}

/// consistently naming function clusters
fn short_name(function_name: &str) -> String {
    let mut h = DefaultHasher::new();
    function_name.hash(&mut h);
    format!("X{:x}", h.finish())
}

/// consistently naming block nodes in function clusters
fn block_name(function_name: &str, id: usize) -> String {
    let mut h = DefaultHasher::new();
    function_name.hash(&mut h);
    format!("X{:x}_{}", h.finish(), id)
}

/// Render statement with context for alloc/type information
fn render_stmt_ctx(s: &Statement, ctx: &GraphContext) -> String {
    use StatementKind::*;
    match &s.kind {
        Assign(p, v) => format!("{} <- {}", p.label(), render_rvalue_ctx(v, ctx)),
        FakeRead(_cause, p) => format!("Fake-Read {}", p.label()),
        SetDiscriminant {
            place,
            variant_index,
        } => format!(
            "set discriminant {}({})",
            place.label(),
            variant_index.to_index()
        ),
        Deinit(p) => format!("Deinit {}", p.label()),
        StorageLive(l) => format!("Storage Live _{}", &l),
        StorageDead(l) => format!("Storage Dead _{}", &l),
        Retag(_retag_kind, p) => format!("Retag {}", p.label()),
        PlaceMention(p) => format!("Mention {}", p.label()),
        AscribeUserType {
            place,
            projections,
            variance: _,
        } => format!("Ascribe {}.{}", place.label(), projections.base),
        Coverage(_) => "Coverage".to_string(),
        Intrinsic(intr) => format!("Intr: {}", render_intrinsic_ctx(intr, ctx)),
        ConstEvalCounter {} => "ConstEvalCounter".to_string(),
        Nop {} => "Nop".to_string(),
    }
}

/// Render rvalue with context
fn render_rvalue_ctx(v: &Rvalue, ctx: &GraphContext) -> String {
    use Rvalue::*;
    match v {
        AddressOf(mutability, p) => match mutability {
            Mutability::Not => format!("&raw {}", p.label()),
            Mutability::Mut => format!("&raw mut {}", p.label()),
        },
        Aggregate(kind, operands) => {
            let os: Vec<String> = operands.iter().map(|op| ctx.render_operand(op)).collect();
            format!("{} ({})", kind.label(), os.join(", "))
        }
        BinaryOp(binop, op1, op2) => format!(
            "{:?}({}, {})",
            binop,
            ctx.render_operand(op1),
            ctx.render_operand(op2)
        ),
        Cast(kind, op, _ty) => format!("Cast-{:?} {}", kind, ctx.render_operand(op)),
        CheckedBinaryOp(binop, op1, op2) => {
            format!(
                "chkd-{:?}({}, {})",
                binop,
                ctx.render_operand(op1),
                ctx.render_operand(op2)
            )
        }
        CopyForDeref(p) => format!("CopyForDeref({})", p.label()),
        Discriminant(p) => format!("Discriminant({})", p.label()),
        Len(p) => format!("Len({})", p.label()),
        Ref(_region, borrowkind, p) => {
            format!(
                "&{} {}",
                match borrowkind {
                    BorrowKind::Mut { kind: _ } => "mut",
                    _other => "",
                },
                p.label()
            )
        }
        Repeat(op, _ty_const) => format!("Repeat {}", ctx.render_operand(op)),
        ShallowInitBox(op, _ty) => format!("ShallowInitBox({})", ctx.render_operand(op)),
        ThreadLocalRef(_item) => "ThreadLocalRef".to_string(),
        NullaryOp(nullop, ty) => format!("{} :: {}", nullop.label(), ty),
        UnaryOp(unop, op) => format!("{:?}({})", unop, ctx.render_operand(op)),
        Use(op) => format!("Use({})", ctx.render_operand(op)),
    }
}

/// Render intrinsic with context
fn render_intrinsic_ctx(intr: &NonDivergingIntrinsic, ctx: &GraphContext) -> String {
    use NonDivergingIntrinsic::*;
    match intr {
        Assume(op) => format!("Assume {}", ctx.render_operand(op)),
        CopyNonOverlapping(c) => format!(
            "CopyNonOverlapping: {} <- {}({})",
            c.dst.label(),
            c.src.label(),
            ctx.render_operand(&c.count)
        ),
    }
}

/// Render terminator with context for alloc/type information
fn render_terminator_ctx(term: &Terminator, ctx: &GraphContext) -> String {
    use TerminatorKind::*;
    match &term.kind {
        Goto { .. } => "Goto".to_string(),
        SwitchInt { discr, .. } => format!("SwitchInt {}", ctx.render_operand(discr)),
        Resume {} => "Resume".to_string(),
        Abort {} => "Abort".to_string(),
        Return {} => "Return".to_string(),
        Unreachable {} => "Unreachable".to_string(),
        Drop { place, .. } => format!("Drop {}", place.label()),
        Call {
            func,
            args,
            destination,
            ..
        } => {
            let fn_name = ctx
                .resolve_call_target(func)
                .map(|n| short_fn_name(&n))
                .unwrap_or_else(|| "?".to_string());
            let arg_str = args
                .iter()
                .map(|op| ctx.render_operand(op))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} = {}({})", destination.label(), fn_name, arg_str)
        }
        Assert {
            cond, expected, ..
        } => format!("Assert {} == {}", ctx.render_operand(cond), expected),
        InlineAsm { .. } => "InlineAsm".to_string(),
    }
}

/// Get target block indices from a terminator
fn terminator_targets(term: &Terminator) -> Vec<usize> {
    use TerminatorKind::*;
    match &term.kind {
        Goto { target } => vec![*target],
        SwitchInt { targets, .. } => {
            let mut result: Vec<usize> = targets.branches().map(|(_, t)| t).collect();
            result.push(targets.otherwise());
            result
        }
        Resume {} | Abort {} | Return {} | Unreachable {} => vec![],
        Drop {
            target, unwind, ..
        } => {
            let mut result = vec![*target];
            if let UnwindAction::Cleanup(t) = unwind {
                result.push(*t);
            }
            result
        }
        Call {
            target, unwind, ..
        } => {
            let mut result = vec![];
            if let Some(t) = target {
                result.push(*t);
            }
            if let UnwindAction::Cleanup(t) = unwind {
                result.push(*t);
            }
            result
        }
        Assert {
            target, unwind, ..
        } => {
            let mut result = vec![*target];
            if let UnwindAction::Cleanup(t) = unwind {
                result.push(*t);
            }
            result
        }
        InlineAsm {
            destination,
            unwind,
            ..
        } => {
            let mut result = vec![];
            if let Some(t) = destination {
                result.push(*t);
            }
            if let UnwindAction::Cleanup(t) = unwind {
                result.push(*t);
            }
            result
        }
    }
}

/// Rendering things as part of graph node labels
trait GraphLabelString {
    fn label(&self) -> String;
}

impl GraphLabelString for Operand {
    fn label(&self) -> String {
        match &self {
            Operand::Constant(ConstOperand { const_, .. }) => {
                let ty = const_.ty();
                match &ty.kind() {
                    stable_mir::ty::TyKind::RigidTy(RigidTy::Int(_))
                    | stable_mir::ty::TyKind::RigidTy(RigidTy::Uint(_)) => {
                        format!("const ?_{}", const_.ty())
                    }
                    _ => format!("const {}", const_.ty()),
                }
            }
            Operand::Copy(place) => format!("cp({})", place.label()),
            Operand::Move(place) => format!("mv({})", place.label()),
        }
    }
}

impl GraphLabelString for Place {
    fn label(&self) -> String {
        project(self.local.to_string(), &self.projection)
    }
}

fn project(local: String, ps: &[ProjectionElem]) -> String {
    ps.iter().fold(local, decorate)
}

fn decorate(thing: String, p: &ProjectionElem) -> String {
    match p {
        ProjectionElem::Deref => format!("(*{})", thing),
        ProjectionElem::Field(i, _) => format!("{thing}.{i}"),
        ProjectionElem::Index(local) => format!("{thing}[_{local}]"),
        ProjectionElem::ConstantIndex {
            offset,
            min_length: _,
            from_end,
        } => format!("{thing}[{}{}]", if *from_end { "-" } else { "" }, offset),
        ProjectionElem::Subslice { from, to, from_end } => {
            format!(
                "{thing}[{}..{}{}]",
                from,
                if *from_end { "-" } else { "" },
                to
            )
        }
        ProjectionElem::Downcast(i) => format!("({thing} as variant {})", i.to_index()),
        ProjectionElem::OpaqueCast(ty) => format!("{thing} as type {ty}"),
        ProjectionElem::Subtype(i) => format!("{thing} :> {i}"),
    }
}

impl GraphLabelString for AggregateKind {
    fn label(&self) -> String {
        use AggregateKind::*;
        match &self {
            Array(_ty) => "Array".to_string(),
            Tuple {} => "Tuple".to_string(),
            Adt(_, idx, _, _, _) => format!("Adt{{{}}}", idx.to_index()), // (AdtDef, VariantIdx, GenericArgs, Option<usize>, Option<FieldIdx>),
            Closure(_, _) => "Closure".to_string(), // (ClosureDef, GenericArgs),
            Coroutine(_, _, _) => "Coroutine".to_string(), // (CoroutineDef, GenericArgs, Movability),
            // CoroutineClosure{} => "CoroutineClosure".to_string(), // (CoroutineClosureDef, GenericArgs),
            RawPtr(ty, Mutability::Mut) => format!("*mut ({})", ty),
            RawPtr(ty, Mutability::Not) => format!("*({})", ty),
        }
    }
}

impl GraphLabelString for Rvalue {
    fn label(&self) -> String {
        use Rvalue::*;
        match &self {
            AddressOf(mutability, p) => match mutability {
                Mutability::Not => format!("&raw {}", p.label()),
                Mutability::Mut => format!("&raw mut {}", p.label()),
            },
            Aggregate(kind, operands) => {
                let os: Vec<String> = operands.iter().map(|op| op.label()).collect();
                format!("{} ({})", kind.label(), os.join(", "))
            }
            BinaryOp(binop, op1, op2) => format!("{:?}({}, {})", binop, op1.label(), op2.label()),
            Cast(kind, op, _ty) => format!("Cast-{:?} {}", kind, op.label()),
            CheckedBinaryOp(binop, op1, op2) => {
                format!("chkd-{:?}({}, {})", binop, op1.label(), op2.label())
            }
            CopyForDeref(p) => format!("CopyForDeref({})", p.label()),
            Discriminant(p) => format!("Discriminant({})", p.label()),
            Len(p) => format!("Len({})", p.label()),
            Ref(_region, borrowkind, p) => {
                format!(
                    "&{} {}",
                    match borrowkind {
                        BorrowKind::Mut { kind: _ } => "mut",
                        _other => "",
                    },
                    p.label()
                )
            }
            Repeat(op, _ty_const) => format!("Repeat {}", op.label()),
            ShallowInitBox(op, _ty) => format!("ShallowInitBox({})", op.label()),
            ThreadLocalRef(_item) => "ThreadLocalRef".to_string(),
            NullaryOp(nullop, ty) => format!("{} :: {}", nullop.label(), ty),
            UnaryOp(unop, op) => format!("{:?}({})", unop, op.label()),
            Use(op) => format!("Use({})", op.label()),
        }
    }
}

impl GraphLabelString for NullOp {
    fn label(&self) -> String {
        match &self {
            NullOp::OffsetOf(_vec) => "OffsetOf(..)".to_string(),
            other => format!("{:?}", other),
        }
    }
}

impl GraphLabelString for NonDivergingIntrinsic {
    fn label(&self) -> String {
        use NonDivergingIntrinsic::*;
        match &self {
            Assume(op) => format!("Assume {}", op.label()),
            CopyNonOverlapping(c) => format!(
                "CopyNonOverlapping: {} <- {}({}))",
                c.dst.label(),
                c.src.label(),
                c.count.label()
            ),
        }
    }
}
