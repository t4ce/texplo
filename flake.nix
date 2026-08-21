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


### to get latest version
# nix run --refresh github:GlassGhost/texplo

### or if you have the latest version
# nix run github:GlassGhost/texplo

### or if you want to submit a patch
# git clone https://github.com/GlassGhost/texplo
# cd ./texplo
### edit the code
# nix build
# nix run
### when you're happy with your patch
### COMMIT BEFORE YOU
# ./make-flake.sh
### Also after make-flake update cargo hashes etc. with
# ./upd8cargo.sh
