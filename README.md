# Rust Stable MIR Pretty Printing

This package provides:

1.  a library crate that provides:
    -   a `rustc` compiler wrapper which can access stable MIR APIs
    -   a pretty-printer for a large fragment of stable MIR
2.  a `rustc` wrapper binary that uses (1)-(2) to pretty-print Rust source files as stable MIR using the `.smir.json` extension.

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

Use the wrapper script `run.sh` (or `cargo run`, but this may also initiate a build).
The options that this tool accepts are identical to `rustc`.
To generate stable MIR output without building a binary, you can invoke the tool as follows:

```shell
./run.sh -Z no-codegen <crate_root>
```

### Invocation Details

We use an uncommon build process where we link against a patched rustc installed in this repo.
However, since `cargo build` does not set `rpath` for dynamic linking, we must manually point the program loader/dynamic linker at the required runtime libraries.
Note that the `cargo run` command appears to prepard the rustlib directories automatically to the dynamic link search path.
If you wish to run the tool manually, you will need to tell the program loader/dynamic linker where to find the missing libraries by:

1.  setting `LD_LIBRARY_PATH`
2.  setting the `rpath` attribute on the binary ELF file
3.  manually invoking the loader (usually `/usr/lib/ld-linux.so.2`) with its specific options
