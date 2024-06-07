# Rust Stable MIR Pretty Printing

This package provides:

1.  a library crate that provides:
    -   a `rustc` compiler wrapper which can access stable MIR APIs
    -   a pretty-printer for a large fragment of stable MIR
2.  a binary crate that uses (1)-(2) to pretty-print Rust source files as stable MIR

It is designed so that anyone can use this library crate as a jumping off point for their own tools which might use stable MIR APIs.

## Building

For first-time builds, run:

```shell
make setup build_all
```

If the underlying `rustc` branch is updated and this crate needs to be rebuilt on top of it, run:

```shell
make update build_all
```

If the source code changes locally for this crate only and it needs to be rebuilt, run:

```shell
make build
```

## Usage

### Running the Tool

TLDR: Run the binary using the wrapper script `run.sh`.

We use an uncommon build process where we link against a patched rustc installed in this repo.
In these cases (when rustc is installed to a non-standard path), the compiler may spuriously becuase required runtime libraries are not found
(typically, these libraries would be picked up ldconfig or the program loader/dynamic linker).
To fix this, we manually copy these non-rust runtime libraries to the installation's rustlib dir;
this way, `cargo build` will pick them up by default.
However, since `cargo build` does not set `rpath` for dynamic linking, we must manually point the program loader/dynamic linker at the required runtime libraries.
Similarly, when executing the tool, `cargo run` command appears to prepard the rustlib directories automatically to the dynamic link search path.
If you wish to run the tool manually, you will need to tell the program loader/dynamic linker where to find the missing libraries by:

1.  setting `LD_LIBRARY_PATH`
2.  setting the `rpath` attribute on the binary ELF file
3.  manually invoking the loader (usually `/usr/lib/ld-linux.so.2`) with its specific options
