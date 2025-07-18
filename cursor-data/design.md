The software `stable-mir-json` consists of Rust code that links to a specific
nightly version of the Rust compiler `rustc`.
The software has a small driver program `driver.rs` which executes the Rust compiler
(with all provided options) and meanwhile calls a specific _hook_ in the compiler
to extract the Middle Intermediate Representation (MIR) into a self-contained JSON file.

For background information about MIR see the following two web pages:
* https://blog.rust-lang.org/2016/04/19/MIR/
* https://rustc-dev-guide.rust-lang.org/mir/index.html

The JSON data serialised in the file is the data structure `crate::printer::SmirJson`.
We call this data the "MIR data".

Apart from extracting the MIR data as JSON into a file, the software can also output a graph 
representation of the extracted MIR in the form of a `*.dot` file for tools from the `graphviz` suite.

The most essential part of the MIR data is the vector of `items`.
Each item in the vector is a Rust function compiled into its MIR, which breaks down the
function body into _basic block_.

The extraction is done using the `stable_mir` crate within the `rustc` software, which
provides a stable API to the compiler's internals.

Besides the `items`, the MIR data in `stable-mir-json` includes a number of _lookup maps_
which are represented by vectors of pairs (`Vec<(Key, Value)>`). 
The tables are additional data which is not part of `stable_mir` data structures.

The `stable_mir` package does not require these tables because it is internal to the compiler
and holds similar lookup tables in an internal state (not accessible directly, only through
the `stable_mir` API functions). `stable-mir-json` has to add this information to the MIR data
to become self-contained. 

This extraction is work in progress; for instance, a known problem
is that not all types used in the Rust program are extracted into the JSON file.
