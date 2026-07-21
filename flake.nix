{
  description = "A Wayland idle management daemon written in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "nothypridle";
          version = cargoToml.workspace.package.version;
          src = self;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
            wayland-scanner
          ];

          buildInputs = with pkgs; [
            wayland
            wayland-protocols
          ];

          postInstall = ''
            install -Dm644 dist/nhidle-cargo.service "$out/lib/systemd/user/nothypridle.service"
            install -Dm644 dist/config.example.aam "$out/share/nothypridle/config.example.aam"
            install -Dm644 dist/rules.example.aam "$out/share/nothypridle/rules.example.aam"
            install -Dm644 dist/schema.aam "$out/share/nothypridle/schema.aam"
          '';

          meta = with pkgs.lib; {
            description = "A Wayland idle management daemon with smart inhibitors";
            homepage = "https://github.com/ininids/nothypridle";
            license = licenses.bsd3;
            mainProgram = "nhidle";
            platforms = platforms.linux;
            maintainers = [];
          };
        };
      });
}
