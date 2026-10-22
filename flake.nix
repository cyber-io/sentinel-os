{
  description = "Sentinel OS - AI-Native Security Operating System";

  inputs = {
    # Core NixOS
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable";

    # Hardware support
    nixos-hardware.url = "github:NixOS/nixos-hardware/master";

    
    
    # Verified boot
    heads.url = "github:osresearch/heads";
    heads.inputs.nixpkgs.follows = "nixpkgs";

    # AI/ML packages
    llama-cpp.url = "github:ggml-org/llama.cpp";
    llama-cpp.inputs.nixpkgs.follows = "nixpkgs";

    
    # Utility flakes
    flake-utils.url = "github:numtide/flake-utils";
    devenv.url = "github:cachix/devenv";
  };

  outputs = { self, nixpkgs, nixpkgs-unstable, nixos-hardware, heads, llama-cpp, flake-utils, devenv, ... }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        # Base pkgs with overlays
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            # llama.cpp overlay
            (import llama-cpp.overlays.default)
            # Local overlays
            (import ./sentinel/overlays)
          ];
          config = {
            allowUnfree = true;
            # Hardening flags for all packages
            hardeningEnable = [ "pie" "bindnow" "fortify" "cfi" "shadowstack" "scut" ];
          };
        };

        # Sentinel-specific pkgs (with AI/ML focus)
        sentinel-pkgs = pkgs.extend (import ./sentinel/pkgs);

        # NixOS configurations
        nixosConfigurations = {
          # Development VM for testing
          dev-vm = nixpkgs.lib.nixosSystem {
            inherit system;
            modules = [
              ({ config, pkgs, ... }: {
                imports = [
                  ./sentinel/modules/base.nix
                  ./sentinel/modules/kernel/hardened.nix
                  ./sentinel/modules/verified-boot.nix
                  ./sentinel/modules/compartments.nix
                  ./sentinel/modules/ai/inference.nix
                  ./sentinel/modules/tools/core.nix
                ];

                # VM-specific settings
                virtualisation.memorySize = 8192;
                virtualisation.cores = 4;
                virtualisation.graphics = true;
                virtualisation.virtioNet = true;
              })
              ./sentinel/configurations/dev-vm.nix
            ];
          };

          # Minimal installer image
          installer = nixpkgs.lib.nixosSystem {
            inherit system;
            modules = [
              ./sentinel/modules/base.nix
              ./sentinel/modules/kernel/hardened.nix
              ./sentinel/modules/verified-boot.nix
              ./sentinel/modules/installer.nix
            ];
          };

          # Production hardened configuration
          hardened = nixpkgs.lib.nixosSystem {
            inherit system;
            modules = [
              ./sentinel/modules/base.nix
              ./sentinel/modules/kernel/hardened.nix
              ./sentinel/modules/verified-boot.nix
              ./sentinel/modules/compartments.nix
              ./sentinel/modules/ai/inference.nix
              ./sentinel/modules/tools/all.nix
              ./sentinel/configurations/hardened.nix
            ];
          };
        };

        # Development shell
        devShells = {
          default = devenv.lib.mkShell {
            packages = with pkgs; [
              git
              nixpkgs-fmt
              statix
              nil
              pre-commit
              # AI/ML tooling
              python312Packages.llama-cpp-python
              python312Packages.torch
              python312Packages.onnxruntime
              # seL4 tooling
              sel4-tools
              # VM tooling
              crosvm
              qemu
            ];
          };
        };

        # CI checks
        checks = {
          # Formatting
          fmt = pkgs.runCommand "fmt" {} ''
            nixpkgs-fmt --check ${self}
          '';
          # Static analysis
          statix = pkgs.runCommand "statix" {} ''
            statix check ${self}
          '';
          # Nix eval
          eval = pkgs.runCommand "eval" {} ''
            nix eval --raw .#nixosConfigurations.dev-vm.config.system.build.toplevel --dry-run
          '';
          # Build dev VM
          build-dev-vm = nixosConfigurations.dev-vm.config.system.build.toplevel;
          # Build installer
          build-installer = nixosConfigurations.installer.config.system.build.isoImage;
        };

        # Apps (for nix run)
        apps = {
          # Sentinel CLI tool
          sentinel-cli = {
            type = "app";
            program = (pkgs.writeScriptBin "sentinel" ''
              #!${pkgs.runtimeShell}
              echo "Sentinel OS CLI - not yet implemented"
              exit 1
            '');
          };
        };
      in
      {
        inherit nixosConfigurations devShells checks apps;
        # Legacy outputs for compatibility
        packages = nixosConfigurations.dev-vm.config.system.build;
        # Export sentinel pkgs for consumers
        sentinel-pkgs = sentinel-pkgs;
      }
    );
}