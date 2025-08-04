{
  callPackage,
  lib,

  makeRustPlatform,
  rustToolchain,
}:

let
  cargoToml = lib.importTOML ../../Cargo.toml;
in
(makeRustPlatform {
  cargo = rustToolchain;
  rustc = rustToolchain;
}).buildRustPackage {
  pname = "stable-mir-json";
  version = cargoToml.package.version;

  src = callPackage ../stable-mir-json-source { };

  cargoLock = {
    lockFile = ../../Cargo.lock;
  };

  RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

  doCheck = false;

  meta = {
    description = "A Rust compiler driver that outputs Stable MIR as JSON";
    homepage = "https://github.com/runtimeverification/stable-mir-json";
    license = lib.licenses.bsd3;
    mainProgram = "stable_mir_json";
  };
}
