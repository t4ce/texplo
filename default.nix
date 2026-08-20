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
  pname = "texplo";
  version = "X";
  # version = "487a6f3d2694f271615c81acd92a5902e0e40223"; # git rev-parse HEAD
  # Points to the source code folder
  src = ./.;
#   src = fetchFromGitHub {
#     owner = "t4ce";
#     repo = pname;
#     rev = version;
#     sha256 = "sha256-9bCx4AHa8q5Wpb+9geDFMG/cYKluDxKtcd2FrEBNhYM=";
#   };

  # buildRustPackage requires one of:
  # cargoHash/cargoSha256/cargoDeps/cargoVendorDir/cargoLock
  cargoHash = "sha256-Ah1aNeuVKFaSEaDbF23SSXKuksBwvpcM9GkgHqK2/BA=";

  preConfigure = ''
    # cargo update
  '';

  outputs = [ "out" ];

  meta = {
    description = "Terminal based file browser in rust.";
    homepage = "https://github.com/trociny/bmkdep";
    license = lib.licenses.bsd2;
    platforms = lib.platforms.unix;
    maintainers = [ lib.maintainers.GlassGhost ];

  };
}
