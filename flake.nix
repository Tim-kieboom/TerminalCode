{
  description = "A lightweight, keyboard-driven IDE for the terminal";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
        "x86_64-darwin"
      ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAll (pkgs: rec {
        terminal_code = pkgs.rustPlatform.buildRustPackage {
          pname = "terminal_code";
          version = (pkgs.lib.importTOML ./Cargo.toml).package.version;
          src = self;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [ pkgs.makeWrapper ];

          # The tests start shells and time frames, which the build sandbox
          # does not suit; CI runs them.
          doCheck = false;

          # The clipboard uses these when they are there; rust-analyzer is
          # found on the PATH when the user has it installed.
          postInstall = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
            wrapProgram $out/bin/trcode \
              --suffix PATH : ${pkgs.lib.makeBinPath [ pkgs.wl-clipboard pkgs.xclip ]}
          '';

          meta = {
            description = "A lightweight, keyboard-driven IDE for the terminal";
            license = [ pkgs.lib.licenses.mit pkgs.lib.licenses.asl20 ];
            mainProgram = "trcode";
          };
        };
        default = terminal_code;
      });

      apps = forAll (pkgs: rec {
        terminal_code = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.terminal_code}/bin/trcode";
        };
        default = terminal_code;
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
          ];
        };
      });
    };
}
