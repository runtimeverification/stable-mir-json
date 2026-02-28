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
//! | [`schema`] | Data model types ([`SmirJson`], [`Item`], [`AllocInfo`], etc.) and type aliases |
//! | [`collect`] | Three-phase pipeline: collect items, analyze bodies, assemble final output |
//! | [`items`] | Constructing [`Item`] values and extracting debug-level details |
//! | [`mir_visitor`] | `BodyAnalyzer`: single-pass MIR traversal collecting calls, allocs, types, spans |
//! | [`ty_visitor`] | Type visitor that recursively collects all reachable types with layout info |
//! | [`link_map`] | Function resolution map: type + instance kind to symbol name |
//! | [`types`] | Type helpers and [`TypeMetadata`](schema::TypeMetadata) construction |
//! | [`util`] | Name resolution, attribute queries, and small collection utilities |

use std::io::Write;
use std::{fs::File, io};

extern crate rustc_middle;
extern crate rustc_session;
extern crate serde_json;

use rustc_middle::ty::TyCtxt;
use rustc_session::config::{OutFileName, OutputType};

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
mod schema;
mod ty_visitor;
mod types;
mod util;

// Re-exports preserving the public API
pub use collect::collect_smir;
pub use items::MonoItemKind;
pub use schema::{AllocInfo, FnSymType, Item, LinkMapKey, SmirJson, TypeMetadata};
pub use util::has_attr;

pub fn emit_smir(tcx: TyCtxt<'_>) {
    let smir_json =
        serde_json::to_string(&collect_smir(tcx)).expect("serde_json failed to write result");

    match tcx.output_filenames(()).path(OutputType::Mir) {
        OutFileName::Stdout => {
            write!(&io::stdout(), "{}", smir_json).expect("Failed to write smir.json");
        }
        OutFileName::Real(path) => {
            let out_path = path.with_extension("smir.json");
            let mut b = io::BufWriter::new(File::create(&out_path).unwrap_or_else(|e| {
                panic!("Failed to create {} output file: {}", out_path.display(), e)
            }));
            write!(b, "{}", smir_json).expect("Failed to write smir.json");
        }
    }
}
