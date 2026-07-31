//! Serialization of Rust's Stable MIR to JSON.
//!
//! This module is the core of `stable-mir-json`: it collects monomorphized items,
//! type metadata, allocations, and span information from the compiler, then
//! serializes them into a [`SmirJson`] structure (emitted as `*.smir.json`).
//!
//! # Module structure
//!
//! | Module | Responsibility |
//! |--------|----------------|
//! | [`schema`] | Data model types ([`SmirJson`], [`Item`], [`AllocInfo`], etc.) and type aliases; [`Item`] deliberately excludes `MonoItem` for structural phase separation |
//! | [`collect`] | Three-phase pipeline: collect items, analyze bodies, assemble final output; phase boundary is enforced structurally via the `(MonoItem, Item)` split |
//! | [`items`] | Constructing `(MonoItem, Item)` pairs and extracting debug-level details |
//! | [`mir_visitor`] | `BodyAnalyzer`: single-pass MIR body traversal collecting calls, allocs, types, spans |
//! | [`ty_visitor`] | `TyCollector`: recursively collects reachable types with layout info (some special kinds are traversed but not stored) |
//! | [`link_map`] | Function resolution map: type + instance kind to symbol name |
//! | [`receipts`] | Spy serializer that discovers interned-index locations; emits `*.smir.receipts.json` (see ADR-004) |
//! | [`types`] | Type helpers and [`TypeMetadata`](schema::TypeMetadata) construction |
//! | [`util`] | Name resolution, attribute queries, and small collection utilities |

use std::io::Write;
use std::{fs::File, io};

use crate::compat::middle::ty::TyCtxt;
use crate::compat::serde_json;

// Macros must be defined before module declarations (textual scoping)
macro_rules! def_env_var {
    ($fn_name:ident, $var_name:ident) => {
        fn $fn_name() -> bool {
            use std::sync::OnceLock;
            static VAR: OnceLock<bool> = OnceLock::new();
            *VAR.get_or_init(|| std::env::var(stringify!($var_name)).is_ok())
        }
    };
}

def_env_var!(debug_enabled, DEBUG);
def_env_var!(link_items_enabled, LINK_ITEMS);
def_env_var!(link_instance_enabled, LINK_INST);

macro_rules! debug_log_println {
    ($($args:tt)*) => {
        #[cfg(feature = "debug_log")]
        println!($($args)*);
    };
}

mod collect;
mod items;
mod link_map;
mod mir_visitor;
pub(crate) mod receipts;
mod schema;
mod ty_visitor;
mod types;
mod util;

// Re-exports preserving the public API
pub use collect::collect_smir;
pub use items::MonoItemKind;
pub use schema::{AllocInfo, FnSymType, Item, LinkMapKey, SmirJson, TypeMetadata};
pub(crate) use util::hash;

pub fn emit_smir(tcx: TyCtxt<'_>) {
    let collected = collect_smir(tcx);

    // Run the spy serializer to discover which JSON paths carry interned
    // indices, then serialize the receipts alongside the main output.
    let receipt = receipts::collect_receipts(&collected);
    let receipt_json =
        serde_json::to_string(&receipt).expect("serde_json failed to write receipts");

    let smir_json = serde_json::to_string(&collected).expect("serde_json failed to write result");

    match crate::compat::output::mir_output_path(tcx, "smir.json") {
        crate::compat::output::OutputDest::Stdout => {
            write!(&io::stdout(), "{smir_json}").expect("Failed to write smir.json");
            // Receipts go to stderr when main output goes to stdout,
            // so they can be captured separately.
            eprintln!("{receipt_json}");
        }
        crate::compat::output::OutputDest::File(path) => {
            let mut b = io::BufWriter::new(
                File::create(&path)
                    .unwrap_or_else(|e| panic!("Failed to create {}: {}", path.display(), e)),
            );
            write!(b, "{smir_json}").expect("Failed to write smir.json");

            // Write the receipts file alongside the JSON output:
            // foo.smir.json → foo.smir.receipts.json
            let receipts_path = path.with_extension("receipts.json");
            let mut rb =
                io::BufWriter::new(File::create(&receipts_path).unwrap_or_else(|e| {
                    panic!("Failed to create {}: {}", receipts_path.display(), e)
                }));
            write!(rb, "{receipt_json}").expect("Failed to write receipts");
        }
    }
}
