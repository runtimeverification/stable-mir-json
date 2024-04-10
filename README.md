# Rust Stable MIR Pretty Printing

This package provides:

1.  a library crate that provides a `rustc` compiler wrapper which can access stable MIR APIs
2.  a pretty-printer for a large fragment of stable MIR
3.  a binary crate that uses (1)-(2) to pretty-print Rust source files as stable MIR

It is designed so that anyone can use this library crate as a jumping off point for their own tools which might use stable MIR APIs.
