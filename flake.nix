{
  description = "stable-mir-json development environment";

  inputs = {
    rv-nix-tools.url = "github:runtimeverification/rv-nix-tools/854d4f05ea78547d46e807b414faad64cea10ae4";
    nixpkgs.follows = "rv-nix-tools/nixpkgs";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils = {
      url = "github:numtide/flake-utils";
    };
  };

  outputs =
    {
      self,
      rv-nix-tools,
      nixpkgs,
      rust-overlay,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        stable-mir-json = pkgs.callPackage ./nix/stable-mir-json {
          inherit rustToolchain;
        };

        stable-mir-json-integration-tests = pkgs.callPackage ./nix/test/integration.nix {
          inherit stable-mir-json;
        };
      in
      {
        packages = {
          inherit stable-mir-json;
          default = stable-mir-json;
          inherit rustToolchain;
        };

        checks = {
          inherit stable-mir-json-integration-tests;
          stable-mir-json-unit-tests = stable-mir-json.overrideAttrs { doCheck = true; };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            zlib
            jq
            gnumake
          ];

          env = {
            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          };
        };
      }
    );
}
