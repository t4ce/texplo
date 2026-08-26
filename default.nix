#################################################################
# official mkDerivation attrs:
  # https://nix.dev/tutorials/callpackage.html
  # https://nixos.org/manual/nixpkgs/stable/#sec-stdenv-phases
  # https://nixos.org/manual/nixpkgs/stable/#var-stdenv-phases
# Default Phase order: 1 unpack, 2 patch, 3 configure, 4 build,
# 5 check, 6 install, 7 fixup, 8 installCheck, 9 dist
# where {$var} is the phase name there is a 
# `pre{$var}`, `{$var}Phase`, and `post{$var}` for each phase
#################################################################

{ lib, stdenv, fetchFromGitHub, rustPlatform, pkg-config, bmake, patchelf, installShellFiles }:
# stdenv.mkDerivation rec {
# pkgs.
rustPlatform.buildRustPackage rec {
  pname = "TerminalDirectoryExplorer";
  version = "X";
  # Points to the source code folder
  src = ./.;
#   src = fetchFromGitHub {
#     owner = "GlassGhost";
#     repo = pname;
#     rev = version;
#     sha256 = "somehash";
#   };

  # buildRustPackage requires one of:
  # cargoHash/cargoSha256/cargoDeps/cargoVendorDir/cargoLock
  cargoHash = "sha256-Ah1aNeuVKFaSEaDbF23SSXKuksBwvpcM9GkgHqK2/BA=";

  preConfigure = ''
    # cargo update
  '';

  outputs = [ "out" ];

  meta = {
    description = "TerminalDirectoryExplorer (TDE) - a fast terminal file browser.";
    homepage = "https://github.com/t4ce/texplo";
    license = lib.licenses.bsd2;
    platforms = lib.platforms.unix;
    mainProgram = "tde";
  };
}

### to get latest version
# nix run --refresh github:t4ce/texplo/otheros

### or if you have the latest version
# nix run github:t4ce/texplo/otheros

### or if you want to submit a patch
# git clone --branch otheros https://github.com/t4ce/texplo
# cd ./texplo
### edit the code
# nix build
# nix run
### when you're happy with your patch
### COMMIT BEFORE YOU
# ./make-flake.sh
### Also after make-flake update cargo hashes etc. with
# ./upd8cargo.sh
