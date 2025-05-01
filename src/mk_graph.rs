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
use stable_mir::ty::{IndexedVal, Ty};
use stable_mir::{
    mir::{
        AggregateKind, BasicBlock, BorrowKind, ConstOperand, Mutability, NonDivergingIntrinsic,
        NullOp, Operand, Place, ProjectionElem, Rvalue, Statement, StatementKind, TerminatorKind,
        UnwindAction,
    },
    ty::RigidTy,
};

use crate::{
    printer::{collect_smir, FnSymType, SmirJson},
    MonoItemKind,
};

// entry point to write the dot file
pub fn emit_dotfile(tcx: TyCtxt<'_>) {
    let smir_dot = collect_smir(tcx).to_dot_file();

    match tcx.output_filenames(()).path(OutputType::Mir) {
        OutFileName::Stdout => {
            write!(io::stdout(), "{}", smir_dot).expect("Failed to write smir.dot");
        }
        OutFileName::Real(path) => {
            let mut b = io::BufWriter::new(
                File::create(path.with_extension("smir.dot"))
                    .expect("Failed to create {path}.smir.dot output file"),
            );
            write!(b, "{}", smir_dot).expect("Failed to write smir.dot");
        }
    }
}

impl SmirJson<'_> {
    pub fn to_dot_file(self) -> String {
        let mut bytes = Vec::new();

        {
            let mut writer = DotWriter::from(&mut bytes);

            writer.set_pretty_print(true);

            let mut graph = writer.digraph();
            graph.set_label(&self.name[..]);
            graph.node_attributes().set_shape(Shape::Rectangle);

            let func_map: HashMap<Ty, String> = self
                .functions
                .into_iter()
                .map(|(k, v)| (k.0, function_string(v)))
                .collect();

            let item_names: HashSet<String> =
                self.items.iter().map(|i| i.symbol_name.clone()).collect();

            // first create all nodes for functions not in the items list
            for f in func_map.values() {
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
                                    b.statements.iter().map(render_stmt).collect();
                                // TODO: render statements and terminator as text label (with line breaks)
                                // switch on terminator kind, add inner and out-edges according to terminator
                                use TerminatorKind::*;
                                match &b.terminator.kind {
                                    Goto { target } => {
                                        label_strs.push("Goto".to_string());
                                        cluster.edge(&this_block, block_name(name, *target));
                                    }
                                    SwitchInt { discr, targets } => {
                                        label_strs.push(format!("SwitchInt {}", discr.label()));
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
                                            cond.label(),
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
                                                    if let Some(callee) = func_map.get(&const_.ty())
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
                                                .map(|op| op.label())
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
fn short_name(function_name: &String) -> String {
    let mut h = DefaultHasher::new();
    function_name.hash(&mut h);
    format!("X{:x}", h.finish())
}

/// consistently naming block nodes in function clusters
fn block_name(function_name: &String, id: usize) -> String {
    let mut h = DefaultHasher::new();
    function_name.hash(&mut h);
    format!("X{:x}_{}", h.finish(), id)
}

fn render_stmt(s: &Statement) -> String {
    use StatementKind::*;
    match &s.kind {
        Assign(p, v) => format!("{} <- {}", p.label(), v.label()),
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
        Intrinsic(intr) => format!("Intr: {}", intr.label()),
        ConstEvalCounter {} => "ConstEvalCounter".to_string(),
        Nop {} => "Nop".to_string(),
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
