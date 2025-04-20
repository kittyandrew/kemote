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

      # @NOTE: I have no idea why this didn't just work..
      nativeBuildInputs = libraries;
      # .. and I had to manually patch/link (?) some libraries, since they are
      # apparently dynamically loaded (?) in some special way that default patch
      # program does not recognize, therefore you have to force it. Sources:
      #   - https://discourse.nixos.org/t/set-ld-library-path-globally-configuration-nix/22281/5
      #   - https://github.com/NixOS/nixpkgs/blob/b024ced1aac25639f8ca8fdfc2f8c4fbd66c48ef/pkgs/by-name/ze/zed-editor/package.nix#L210-L211
      postInstall = ''
        patchelf --add-rpath ${pkgs.libxkbcommon}/lib $out/bin/kemote
        patchelf --add-needed ${pkgs.wayland}/lib/libwayland-client.so $out/bin/kemote
        patchelf --add-needed ${pkgs.vulkan-loader}/lib/libvulkan.so.1 $out/bin/kemote
      '';
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
