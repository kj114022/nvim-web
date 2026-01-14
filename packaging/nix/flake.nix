{
  description = "Neovim in the Browser - Real Neovim via WebSocket/WebTransport";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };
      in
      {
        packages = {
          default = self.packages.${system}.nvim-web;

          nvim-web = pkgs.rustPlatform.buildRustPackage {
            pname = "nvim-web";
            version = "0.1.0";

            src = ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            buildInputs = with pkgs; [
              openssl
            ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              pkgs.darwin.apple_sdk.frameworks.Security
              pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
            ];

            nativeBuildInputs = with pkgs; [
              pkg-config
            ];

            cargoBuildFlags = [ "-p" "nvim-web-host" ];

            meta = with pkgs.lib; {
              description = "Neovim in the Browser";
              homepage = "https://github.com/kj114022/nvim-web";
              license = licenses.mit;
              maintainers = [ ];
              mainProgram = "nvim-web-host";
            };
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
            openssl
            neovim
            wasm-pack
            nodejs
            ripgrep

            # Development tools
            rust-analyzer
            cargo-watch
            cargo-audit
          ];

          shellHook = ''
            echo "nvim-web development environment"
            echo "Rust: $(rustc --version)"
            echo "Neovim: $(nvim --version | head -1)"
          '';
        };

        apps.default = {
          type = "app";
          program = "${self.packages.${system}.nvim-web}/bin/nvim-web-host";
        };
      }
    );
}
