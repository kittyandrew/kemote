# Original source: https://github.com/SergioRibera/gpui_nix_examples/blob/main/flake.nix
# @TODO: Make this the new rust template. its just better.
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    fenix.url = "github:nix-community/fenix";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    nixpkgs,
    flake-utils,
    crane,
    ...
  } @ inputs:
  # Iterate over Arm, x86 for MacOs 🍎 and Linux 🐧
  (flake-utils.lib.eachDefaultSystem (system: let
    pkgs = nixpkgs.legacyPackages.${system};

    toolchain = inputs.fenix.packages.${system}.minimal.toolchain;
    craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

    buildInputs = with pkgs; [
      pkg-config
      openssl.dev # to build gpui and reqwests
      wayland # to build gpui
    ];

    src = pkgs.lib.cleanSourceWith {
      src = craneLib.path ./.;
    };

    libraries = with pkgs; [
      openssl
      vulkan-loader
      libxkbcommon
      wayland
      xorg.libX11
    ];
  in {
    # nix build
    packages.default = craneLib.buildPackage {
      doCheck = false;
      inherit src buildInputs; # afaiu version is inherited from Cargo.toml
      nativeBuildInputs = libraries;
    };

    # nix develop
    devShells.default = craneLib.devShell {
      inherit buildInputs;
      packages = [toolchain] ++ libraries;
      LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath libraries}";
      CARGO_PROFILE_DEV_BUILD_OVERRIDE_DEBUG = true;
    };
  }));
}
