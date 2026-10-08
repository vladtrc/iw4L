{
  description = "iw4L development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
    }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ] (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        runtimeLibs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux (
          with pkgs;
          [
            alsa-lib
            libxkbcommon
            udev
            vulkan-loader
            wayland
            libx11
            libxcursor
            libxi
            libxrandr
          ]
        );
      in
      {
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            toolchain
            pkg-config
            cmake
            clang
            git
          ];
          buildInputs = runtimeLibs;
          LD_LIBRARY_PATH = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux (
            pkgs.lib.makeLibraryPath runtimeLibs
          );
        };
      }
    );
}
