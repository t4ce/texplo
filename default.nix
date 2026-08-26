{ lib, rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "termdir";
  version = "0.14.0";
  src = ./.;

  cargoLock.lockFile = ./Cargo.lock;

  meta = {
    description = "Fast terminal directory explorer";
    homepage = "https://github.com/t4ce/texplo";
    license = lib.licenses.bsd2;
    platforms = lib.platforms.unix;
    mainProgram = "td";
  };
}
