{
  lib,
  stdenv,
  callPackage,

  jq,
  gnumake,
  stable-mir-json,
}:

let
  cargoToml = lib.importTOML ../../Cargo.toml;
in
stdenv.mkDerivation {
  pname = "stable-mir-json-tests";
  version = cargoToml.package.version;

  src = callPackage ../stable-mir-json-source { };

  nativeBuildInputs = [
    stable-mir-json
    jq
    gnumake
  ];

  buildPhase = ''
    export SMIR="stable_mir_json -Zno-codegen"

    make integration-test
  '';

  installPhase = ''
    mkdir -p $out
  '';

  meta = {
    description = "Integration tests for stable-mir-json";
  };
}
