let
  sources = import ./nix/sources.nix;
  rust-overlay = import sources.rust-overlay;
  pkgs = import sources.nixpkgs { overlays = [ rust-overlay ]; };

  rust = pkgs.rust-bin.stable.latest.default.override {
    extensions = [ "rust-src" ];
  };
in
pkgs.mkShell {
  buildInputs = [
    pkgs.niv
    # rust and profiling
    rust
    pkgs.cargo-flamegraph
    pkgs.lld
    pkgs.llvmPackages.bintools
    pkgs.tracy
    pkgs.renderdoc
    # asset authoring
    pkgs.blender
    pkgs.tiled
    pkgs.jq
    pkgs.entr
    pkgs.just
    # wgpu C dependencies
    pkgs.pkgconfig
    pkgs.xorg.libX11
  ];
  # make C libraries available
  LD_LIBRARY_PATH = with pkgs.xorg; with pkgs.lib.strings;
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
