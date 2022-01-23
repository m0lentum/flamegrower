let
  sources = import ./nix/sources.nix;

  pkgs = import sources.nixpkgs { overlays = [ (import sources.nixpkgs-mozilla) ]; };

  rust = (pkgs.rustChannelOf { channel = "1.56.1"; }).rust.override {
    extensions = [ "rust-src" ];
  };
in
pkgs.mkShell {
  buildInputs = [
    pkgs.niv
    rust
    pkgs.cargo-flamegraph
    pkgs.lld
    pkgs.llvmPackages.bintools
    pkgs.tracy
    # asset authoring tools and utilities
    pkgs.tiled
    pkgs.jq
    pkgs.entr
    pkgs.just
    # for wgpu
    pkgs.pkgconfig
    pkgs.xlibs.libX11
  ];
  # make libraries available
  LD_LIBRARY_PATH = with pkgs.xlibs; with pkgs.lib.strings;
    concatStrings (intersperse ":" [
      "${libX11}/lib"
      "${libXcursor}/lib"
      "${libXxf86vm}/lib"
      "${libXi}/lib"
      "${libXrandr}/lib"
      "${pkgs.vulkan-loader}/lib"
      "${pkgs.stdenv.cc.cc.lib}/lib64"
    ]);
}
