{
  description = "Texplo - Terminal based file browser in rust.";
  inputs = {
    nixpkgs.url = "nixpkgs";
    flake-parts.url = "github:hercules-ci/flake-parts";
  };
  outputs = inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      perSystem = { system, pkgs, ... }:
        let
          pkg = pkgs.callPackage ./default.nix { };
        in {
          packages.default = pkg;
          devShells.default = pkgs.mkShell { packages = [ pkg ]; };
        };
    };
}