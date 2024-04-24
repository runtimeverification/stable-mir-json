mod common;
use common::*;
use smir_pretty::{stable_mir_driver, print_all_items_verbose};

#[test]
fn test_pretty_print() {
    stable_mir_driver(& vec!["rustc".into(), get_resource_path(vec!["tests", "resources", "println.rs"])], print_all_items_verbose);
}
