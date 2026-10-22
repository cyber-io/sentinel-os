{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.sentinel;
in
{
  options.sentinel = {
    enable = mkEnableOption "Sentinel OS core";

    # Hardware platform
    platform = mkOption {
      type = types.enum [ "x86_64" "aarch64" "riscv64" ];
      default = "x86_64";
      description = "Target hardware platform";
    };

    # Security level
    securityLevel = mkOption {
      type = types.enum [ "standard" "hardened" "paranoid" ];
      default = "hardened";
      description = "Security hardening level";
    };

    # AI/ML features
    ai = {
      enable = mkEnableOption "AI/ML inference stack";
      gpuSupport = mkEnableOption "GPU acceleration for inference";
      modelsPath = mkOption {
        type = types.path;
        default = "/var/lib/sentinel/models";
        description = "Path to store AI models";
      };
    };

    # Compartment runtime
    compartments = {
      enable = mkEnableOption "seL4-based compartment runtime";
      backend = mkOption {
        type = types.enum [ "crosvm" "qemu" "kata" ];
        default = "crosvm";
        description = "VMM/container backend";
      };
    };

    # Tool selection profile
    tools = {
      profile = mkOption {
        type = types.enum [ "minimal" "core" "full" "forensics" "wireless" "cloud" ];
        default = "core";
        description = "Pre-defined tool profile";
      };
      custom = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "Additional tools to include";
      };
    };
  };

  config = mkIf cfg.enable {
    # Base system configuration
    boot = {
      # Kernel will be configured in kernel/hardened.nix
      kernelModules = [ "vfio" "vfio_pci" "vfio_iommu_type1" "kvm" "kvm_intel" "kvm_amd" ];
      initrd.kernelModules = [ "vfio" "vfio_pci" ];

      # Loader configuration
      loader = {
        systemd-boot.enable = true;
        efi.canTouchEfiVariables = true;
        # Secure boot will be configured in verified-boot.nix
      };
    };

    # File systems - immutable root with mutable overlays
    fileSystems."/" = {
      fsType = "btrfs";
      options = [ "defaults" "noatime" "compress=zstd:3" "subvol=@" ];
    };

    # Btrfs subvolumes for atomic updates
    btrfs = {
      enable = true;
      subvolumes = {
        "@" = { path = "/"; };
        "@home" = { path = "/home"; };
        "@var-lib-sentinel" = { path = "/var/lib/sentinel"; };
        "@var-log" = { path = "/var/log"; };
        "@nix-store" = { path = "/nix/store"; options = [ "noatime" "nodatacow" ]; };
      };
    };

    # Networking - locked down by default
    networking = {
      hostName = "sentinel";
      # No default network services
      firewall = {
        enable = true;
        allowPing = false;
        logRejectedPackets = true;
        logRejectedConnections = true;
        # Rules added by compartments
      };
      # Disable unnecessary services
      disableIPv6 = false; # Keep for modern networking
    };

    # Systemd hardening
    systemd = {
      # Protect all services by default
      protectKernelTunables = true;
      protectKernelModules = true;
      protectControlGroups = true;
      restrictNamespaces = true;
      lockPersonality = true;
      memoryDenyWriteExecute = true;
      restrictAddressFamilies = [ "AF_UNIX" "AF_INET" "AF_INET6" ];
      restrictRealtime = true;
      systemCallFilter = [ "@system-service" "@clock" "@cpu-emulation" ];
      systemCallFilterMask = [ "@obsolete" "@debug" "@module" "@raw-io" "@reboot" "@swap" ];
      # Private tmp for all services
      privateTmp = true;
      privateDevices = true;
      protectHome = true;
      protectSystem = "strict";
      readOnlyPaths = [ "/etc" "/usr" "/boot" ];
      inaccessiblePaths = [ "/root" "/home/*/.ssh" ];
    };

    # User management
    users.users.sentinel = {
      isNormalUser = true;
      description = "Sentinel operator";
      extraGroups = [ "wheel" "kvm" "render" "video" ];
      # SSH keys added via configuration
    };

    # Sudo with logging
    security.sudo = {
      enable = true;
      configFile = ''
        Defaults log_output, log_input
        Defaults logfile="/var/log/sudo.log"
        Defaults requiretty
        %wheel ALL=(ALL) ALL
      '';
    };

    # Audit logging
    security.audit = {
      enable = true;
      rules = ''
        # Track all execve
        -a always,exit -F arch=b64 -S execve -k exec
        # Track file modifications in sensitive areas
        -w /etc -p wa -k etc_changes
        -w /boot -p wa -k boot_changes
        -w /nix/store -p wa -k store_changes
        # Track capability changes
        -a always,exit -F arch=b64 -S capset -k caps
        # Track network connections
        -a always,exit -F arch=b64 -S connect -k network
        -a always,exit -F arch=b64 -S bind -k network
      '';
    };

    # Kernel lockdown (enforced in hardened.nix)
    security.lockKernelModules = true;

    # TPM support
    hardware.tpm2 = {
      enable = true;
      tcsd.enable = true;
    };

    # Entropy
    services.haveged = {
      enable = true;
      config = { "write-wakeup-threshold" = 1024; };
    };

    # Nix configuration
    nix = {
      # Binary cache
      substituters = [
        "https://cache.nixos.org"
        "https://sentinel-cache.example.org"  # To be configured
      ];
      trustedPublicKeys = [
        "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
        "sentinel-cache.example.org:..."  # To be configured
      ];
      # Sandboxed builds
      sandbox = true;
      # Experimental features
      extraOptions = ''
        experimental-features = nix-command flakes pipe-operators
        auto-optimise-store = true
        max-jobs = auto
        cores = 0
      '';
    };

    # Time synchronization
    services.timesyncd = {
      enable = true;
      config.NTP = [ "0.pool.ntp.org" "1.pool.ntp.org" ];
    };

    # Logging
    services.journald = {
      extraConfig = {
        Storage = "persistent";
        Compress = "yes";
        ForwardToSyslog = "no";
        MaxLevelStore = "debug";
        SystemMaxUse = "2G";
        RuntimeMaxUse = "500M";
      };
    };

    # SSH - key-only, hardened
    services.openssh = {
      enable = true;
      settings = {
        PermitRootLogin = "no";
        PasswordAuthentication = "no";
        PubkeyAuthentication = "yes";
        PermitEmptyPasswords = "no";
        ChallengeResponseAuthentication = "no";
        UsePAM = "yes";
        AuthenticationMethods = "publickey";
        PubkeyAcceptedAlgorithms = "ssh-ed25519,ssh-ed25519-cert-v01@openssh.com";
        HostKeyAlgorithms = "ssh-ed25519,ssh-ed25519-cert-v01@openssh.com";
        KexAlgorithms = "curve25519-sha256,curve25519-sha256@libssh.org";
        Ciphers = "chacha20-poly1305@openssh.com,aes256-gcm@openssh.com";
        MACs = "hmac-sha2-256-etm@openssh.com,hmac-sha2-512-etm@openssh.com";
        ClientAliveInterval = 300;
        ClientAliveCountMax = 2;
        MaxAuthTries = 3;
        LoginGraceTime = 30;
        Banner = "/etc/ssh/banner";
      };
    };

    # Environment
    environment = {
      systemPackages = with pkgs; [
        # Core utilities
        bash
        coreutils
        findutils
        grep
        gnugrep
        less
        vim
        git
        # System inspection
        htop
        btop
        iotop
        lsof
        strace
        perf
        # Network
        iproute2
        net-tools
        nmap-ncat
        socat
        wireshark-cli
        # Crypto
        gnupg
        age
        sops
        # Hardware
        lm_sensors
        smartmontools
        dmidecode
        hwloc
        # TPM
        tpm2-tools
        tpm2-tss
      ];
    };

    # Enable flakes
    nix.settings.experimental-features = [ "nix-command" "flakes" ];

    # System packages for all profiles
    environment.systemPackages = with pkgs; [
      # Sentinel-specific tools (will be populated by tools modules)
    ];
  };
}