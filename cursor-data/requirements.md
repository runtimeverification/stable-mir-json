requirements.md

The software `stable-mir-json` should

* compile any given Rust program in the same way as the underlying nightly `rustc` version would
* not crash on an attempt to compile any Rust program
* faithfully extract all MIR data of a given Rust program into a JSON representation which contains _no external references_ (with the exception of references to other crates that the Rust program is declared to depend on).
* output the JSON for its MIR data in a compact form for space efficiency
* sort all lookup tables of type `Vec<(Key, Value)>` in the MIR data by the respective `Key`s to facilitate reading and comparing by humans
