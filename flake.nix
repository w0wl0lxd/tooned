{
  description = "tooned - transparent TOON re-encoding for AI coding agent tool-call context";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        # `nix develop` (or direnv's `use flake`) drops you into a shell with
        # `mise` and `rustup` on PATH. Deliberately NOT pinning an rustc/cargo
        # *version* here via nixpkgs: `rust-toolchain.toml` (rustup convention)
        # is the single source of truth for the Rust version, already used by
        # CI (dtolnay/rust-toolchain) and by contributors without Nix at all.
        # `rustup` just needs to be *present* so its `cargo`/`rustc` proxies
        # can read that file and lazily install `stable` on first use --
        # nixpkgs' own `rustc`/`cargo` derivations are intentionally avoided,
        # since that would create a second, competing source of truth that
        # can drift out of sync with rust-toolchain.toml.
        devShells.default = pkgs.mkShellNoCC {
          packages = [
            pkgs.mise
            pkgs.rustup
            pkgs.pkg-config
          ];

          shellHook = ''
            eval "$(mise activate bash)"
          '';
        };
      }
    );
}
