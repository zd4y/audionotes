{
  description = "audionotes";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        cargoToml = with builtins; (fromTOML (readFile ./Cargo.toml));
        nativeBuildInputs = [ pkgs.pkg-config ];
        buildInputs = [ pkgs.openssl ];
      in
      {
        packages = {
          audionotes = pkgs.rustPlatform.buildRustPackage {
            inherit nativeBuildInputs buildInputs;
            inherit (cargoToml.package) version;
            pname = cargoToml.package.name;
            src = pkgs.lib.cleanSource ./.;
            doCheck = true;
            cargoLock.lockFile = ./Cargo.lock;
            meta.mainProgram = "audionotes";
          };
          dockerImage = pkgs.dockerTools.buildImage {
            name = "audionotes";
            config = {
              Cmd = [ pkgs.lib.getExe self.packages.${system}.audionotes ];
            };
          };
          default = self.packages.${system}.audionotes;
        };

        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs;
          buildInputs = buildInputs ++ [ pkgs.flyctl pkgs.sqlx-cli ];
        };
      }
    );
}
