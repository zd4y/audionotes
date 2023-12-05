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
        name = cargoToml.package.name;
        nativeBuildInputs = [ pkgs.pkg-config ];
        buildInputs = [ pkgs.openssl pkgs.makeWrapper ];
      in
      {
        packages = {
          audionotes = pkgs.rustPlatform.buildRustPackage {
            inherit nativeBuildInputs buildInputs;
            inherit (cargoToml.package) version;
            pname = name;
            src = pkgs.lib.cleanSource ./.;
            doCheck = true;
            cargoLock.lockFile = ./Cargo.lock;
            postInstall = ''
              wrapProgram $out/bin/${name} --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.ffmpeg ]}
            '';
            meta.mainProgram = name;
          };
          dockerImage = pkgs.dockerTools.buildImage {
            name = "audionotes";

            copyToRoot = pkgs.buildEnv {
              name = "image-root";
              paths = [ pkgs.cacert ];
            };

            config = {
              Cmd = [ (pkgs.lib.getExe self.packages.${system}.audionotes) ];
            };
          };
          default = self.packages.${system}.audionotes;
        };

        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs;
          buildInputs = buildInputs ++ [ pkgs.sqlx-cli ];
        };
      }
    );
}
