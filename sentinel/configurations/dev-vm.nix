{ config, lib, pkgs, ... }:

with lib;

{
  # Development VM configuration
  # Optimized for rapid iteration, debugging, and testing

  # Use standard kernel for faster builds (hardened for production)
  boot.kernelPackages = pkgs.linuxPackages_latest;

  # Less aggressive hardening for development
  boot.kernelParams = [
    "kpti=on"
    "spectre_v2=on"
    "spec_store_bypass_disable=on"
    "module.sig_enforce=0"  # Allow unsigned modules for development
    "lockdown=none"         # Disable lockdown for debugging
  ];

  # Enable debugging features
  boot.kernel.sysctl = {
    "kernel.perf_event_paranoid" = -1;
    "kernel.kptr_restrict" = 0;
    "kernel.dmesg_restrict" = 0;
    "kernel.yama.ptrace_scope" = 0;
    "kernel.unprivileged_bpf_disabled" = 0;
  };

  # Enable KVM for nested virtualization
  boot.kernelModules = [ "kvm" "kvm_intel" "kvm_amd" "vfio" "vfio_pci" ];

  # VirtualBox guest additions (if running in VirtualBox)
  virtualisation.virtualbox.guest.enable = true;
  virtualisation.virtualbox.guest.enable = true;

  # QEMU guest agent
  services.qemu-guest-agent.enable = true;

  # Development tools
  environment.systemPackages = with pkgs; [
    # Editors
    neovim
    helix
    vscode
    # Debugging
    gdb
    lldb
    rr
    valgrind
    # Profiling
    perf
    hotspot
    flamegraph
    # Tracing
    bpftrace
    bcc
    # Network
    wireshark
    tshark
    # Containers
    docker
    docker-compose
    podman
    buildah
    skopeo
    # CI/CD
    gitlab-runner
    # Testing
    bats
    shellcheck
    shfmt
    # Documentation
    mkdocs
    mkdocs-material
  ];

  # Enable Docker
  virtualisation.docker.enable = true;

  # User namespace for rootless containers
  boot.kernelParams = [ "user_namespace.enable=1" ];

  # SSH for remote development
  services.openssh.enable = true;
  services.openssh.settings.PasswordAuthentication = true; # For dev only

  # Allow unfree for development tools
  nixpkgs.config.allowUnfree = true;

  # Auto-login for development
  services.getty.autologinUser = "sentinel";

  # Disable some hardening for development
  security.lockKernelModules = false;

  # Enable NixOS flakes
  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  # Faster builds
  nix.settings.max-jobs = "auto";
  nix.settings.cores = 0;

  # Garbage collection
  nix.gc = {
    automatic = true;
    dates = "weekly";
    options = "--delete-older-than 30d";
  };

  # Development environment
  users.users.sentinel = {
    extraGroups = [ "wheel" "kvm" "render" "video" "docker" "libvirt" "wireshark" ];
    shell = pkgs.zsh;
  };

  # Zsh with oh-my-zsh
  programs.zsh = {
    enable = true;
    ohMyZsh = {
      enable = true;
      theme = "agnoster";
      plugins = [ "git" "docker" "kubectl" "nix" "rust" "python" ];
    };
  };

  # Git configuration
  programs.git = {
    enable = true;
    userName = "Sentinel Developer";
    userEmail = "dev@sentinel-os.local";
    signing = {
      signByDefault = true;
      gpgSigningKey = ""; # Set per-user
    };
  };

  # Direnv for project-specific environments
  programs.direnv.enable = true;

  # Nix flake development
  nix.devTools = true;

  # Language servers
  environment.systemPackages = with pkgs; [
    nil
    python312Packages.python-lsp-server
    python312Packages.ruff-lsp
    typescript-language-server
    yaml-language-server
    dockerfile-language-server-nodejs
    bash-language-server
    lua-language-server
    rust-analyzer
    gopls
    clang-tools
  ];
}