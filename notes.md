# `stable_mir_json` packaged, `cargo build` with 1 file
```
daniel@daniel-MS-7E06 example1$ cargo clean
     Removed 22 files, 7.8MiB total
daniel@daniel-MS-7E06 example1$ cat src/main.rs 
fn main() {
    println!("Hello, world!");
}
daniel@daniel-MS-7E06 example1$ LD_LIBRARY_PATH="/home/daniel/Applications/stable_mir_json/target/debug/deps:/home/daniel/Applications/stable_mir_json/target/debug:/home/daniel/.rustup/toolchains/nightly-2024-11-29-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/lib" RUSTC=~/.stable_mir_json/debug/stable_mir_json cargo build
   Compiling example1 v0.1.0 (/home/daniel/example1)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.06s
daniel@daniel-MS-7E06 example1$ find . -name "*smir*"
./target/debug/deps/example1-62ce7b20115458ac.smir.json
```

# `stable_mir_json` packaged, `cargo build` with a multi crate workspace (WITH kani adjusted LD_LIBRARY_PATH)
```
daniel@daniel-MS-7E06 example2$ cat Cargo.toml 
[workspace]
members = ["inner_a", "inner_b", "runner"]
resolver = "3"

daniel@daniel-MS-7E06 example2$ cat inner_a/src/lib.rs 
pub fn hello_a() {
    println!("hello from inner_a")
}
daniel@daniel-MS-7E06 example2$ cat inner_b/src/lib.rs 
use inner_a::hello_a;


pub fn hello_b() {
	hello_a();
    println!("hello from inner_b")
}

daniel@daniel-MS-7E06 example2$ cat runner/src/main.rs 
use inner_b::hello_b;

fn main() {
    hello_b();
}
daniel@daniel-MS-7E06 example2$ LD_LIBRARY_PATH="/home/daniel/Applications/stable_mir_json/target/debug/deps:/home/daniel/Applications/stable_mir_json/target/debug:/home/daniel/.rustup/toolchains/nightly-2024-11-29-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/lib" RUSTC=~/.stable_mir_json/debug/stable_mir_json cargo build
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.00s
daniel@daniel-MS-7E06 example2$ cargo clean
     Removed 49 files, 7.9MiB total
daniel@daniel-MS-7E06 example2$ LD_LIBRARY_PATH="/home/daniel/Applications/stable_mir_json/target/debug/deps:/home/daniel/Applications/stable_mir_json/target/debug:/home/daniel/.rustup/toolchains/nightly-2024-11-29-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/lib" RUSTC=~/.stable_mir_json/debug/stable_mir_json cargo build
   Compiling inner_a v0.1.0 (/home/daniel/example2/inner_a)
   Compiling inner_b v0.1.0 (/home/daniel/example2/inner_b)
   Compiling runner v0.1.0 (/home/daniel/example2/runner)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.10s
daniel@daniel-MS-7E06 example2$ find . -name "*smir*"
./target/debug/deps/runner-20bce5e7223da426.smir.json
./target/debug/deps/inner_b-d70d66d12c6bb36c.smir.json
./target/debug/deps/inner_a-4d24ee92b46ed223.smir.json
```
# `stable_mir_json` packaged, `cargo build` with a multi crate workspace (WITHOUT kani adjusted LD_LIBRARY_PATH)
```
daniel@daniel-MS-7E06 example2$ rm -rf $(find . -name "*smir*")
daniel@daniel-MS-7E06 example2$ cargo clean
     Removed 49 files, 7.9MiB total
daniel@daniel-MS-7E06 example2$ LD_LIBRARY_PATH="/home/daniel/Applications/stable_mir_json/target/debug/deps:/home/daniel/Applications/stable_mir_json/target/debug:/home/daniel/.rustup/toolchains/nightly-2024-11-29-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/lib:/home/daniel/.rustup/toolchains/nightly-2024-11-29-x86_64-unknown-linux-gnu/lib" RUSTC=~/.stable_mir_json/debug/stable_mir_json cargo build
   Compiling inner_a v0.1.0 (/home/daniel/example2/inner_a)
   Compiling inner_b v0.1.0 (/home/daniel/example2/inner_b)
   Compiling runner v0.1.0 (/home/daniel/example2/runner)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.18s
daniel@daniel-MS-7E06 example2$ find . -name "*smir*"
./target/debug/deps/runner-20bce5e7223da426.smir.json
./target/debug/deps/inner_b-d70d66d12c6bb36c.smir.json
./target/debug/deps/inner_a-4d24ee92b46ed223.smir.json
```