{
  callPackage,
  lib,
  stdenv,

  makeRustPlatform,
  zlib,

  rustToolchain,
}:

let
  cargoToml = lib.importTOML ../../Cargo.toml;
in
(makeRustPlatform {
  cargo = rustToolchain;
  rustc = rustToolchain;
}).buildRustPackage
  {
    pname = "stable-mir-json";
    version = cargoToml.package.version;

    src = callPackage ../stable-mir-json-source { };

    cargoLock = {
      lockFile = ../../Cargo.lock;
    };

    nativeBuildInputs = [
      zlib
    ];

    preFixup = lib.optionalString stdenv.hostPlatform.isDarwin ''
      install_name_tool -add_rpath "${rustToolchain}/lib" "$out/bin/stable_mir_json"
    '';

    RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

    doCheck = false;

    meta = {
      description = "A Rust compiler driver that outputs Stable MIR as JSON";
      homepage = "https://github.com/runtimeverification/stable-mir-json";
      license = lib.licenses.bsd3;
      mainProgram = "stable_mir_json";
    };
  }
