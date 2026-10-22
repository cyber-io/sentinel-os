{ config, lib, pkgs, ... }:

with lib;

{
  # Installer configuration - minimal, verified boot ready

  # Use hardened kernel for installer
  imports = [ ./../modules/kernel/hardened.nix ];

  # Minimal base (no compartments, no AI)
  imports = [ ./../modules/base.nix ];

  # Verified boot for installer
  imports = [ ./../modules/verified-boot.nix ];

  # ============ INSTALLER SPECIFIC ============

  # Installation media
  system.build.isoImage = {
    # ISO label
    label = "SENTINEL_OS_INSTALLER";
    # Publisher
    publisher = "Sentinel OS Project";
    # Application ID
    applicationId = "Sentinel OS Installer";
    # Version
    version = "0.1.0";
  };

  # Boot loader for ISO
  boot.loader = {
    # systemd-boot for UEFI
    systemd-boot.enable = true;
    efi = {
      canTouchEfiVariables = false; # Read-only on ISO
    };
    # GRUB for BIOS compatibility (optional)
    grub.enable = true;
    grub.devices = [ ];
    grub.efiSupport = true;
    grub.efiInstallAsRemovable = true;
  };

  # Kernel command line for installer
  boot.kernelParams = [
    "boot=live"
    "union=overlay"
    "toram"  # Load to RAM for faster install
    "quiet"
    "loglevel=3"
    "rd.luks=0"
    "rd.lvm=0"
    "rd.md=0"
    "rd.dm=0"
    # Security
    "module.sig_enforce=1"
    "lockdown=confidentiality"
    "kpti=on"
    "spectre_v2=on"
    "spec_store_bypass_disable=on"
    "tsx=off"
    "mds=full,nosmt"
    "l1tf=full,force"
    "randomize_kstack_offset=on"
    "kaslr"
    "slab_nomerge"
    "init_on_alloc=1"
    "init_on_free=1"
    "page_poison=1"
    "vsyscall=none"
  ];

  # Initrd for installer
  boot.initrd = {
    availableKernelModules = [ "virtio_blk" "virtio_scsi" "virtio_net" "nvme" "ahci" "usb_storage" "sd_mod" "sr_mod" "loop" "squashfs" "overlay" ];
    kernelModules = [ "virtio_blk" "virtio_scsi" "nvme" "ahci" ];
    luks = {
      enable = true;
      support = true;
    };
    lvm = true;
    mdadm = true;
    dmraid = true;
    zfs = false;
    btrfs = true;
    # Network for netboot
    network.enable = true;
    network.activate = true;
    network.ssh.enable = true;
    network.ssh.port = 22;
    # SSH keys for installer access
    network.ssh.hostKeys = [ ];
    network.ssh.authorizedKeys = [ ]; # Will be injected
    # Compression
    compress = true;
    lz4 = true;
  };

  # Minimal packages for installer
  environment.systemPackages = with pkgs; [
    # Core
    bash
    coreutils
    findutils
    grep
    gnugrep
    gawk
    sed
    less
    vim
    # Installer
    calamares
    calamares-modules
    # Partitioning
    parted
    gdisk
    fdisk
    cfdisk
    sgdisk
    # Filesystems
    btrfs-progs
    dosfstools
    e2fsprogs
    xfsprogs
    f2fs-tools
    cryptsetup
    lvm2
    mdadm
    dmraid
    # Network
    iproute2
    net-tools
    nmap-ncat
    socat
    dhcpcd
    networkmanager
    networkmanager-l2tp
    networkmanager-openvpn
    networkmanager-wireguard
    networkmanager-strongswan
    # WiFi
    wpa_supplicant
    iw
    wireless-tools
    # Hardware
    lsusb
    lspci
    dmidecode
    hwloc
    smartmontools
    # TPM
    tpm2-tools
    tpm2-tss
    # Crypto
    gnupg
    age
    sops
    openssl
    # Verification
    sbctl
    cosign
    # Nix
    nix
    nixos-install-tools
    nixos-generate-config
    # Boot
    efibootmgr
    grub2
    systemd-boot
    # Monitoring
    htop
    btop
    lsof
    # Compression
    gzip
    bzip2
    xz
    zstd
    tar
    unzip
    # Python for scripts
    python312
    python312Packages.requests
    python312Packages.pyyaml
    python312Packages.click
  ];

  # Calamares configuration
  services.calamares = {
    enable = true;
    settings = {
      branding = "sentinel";
      # Modules
      modules = [
        "welcome"
        "locale"
        "keyboard"
        "partition"
        "users"
        "summary"
        "install"
        "bootloader"
        "finished"
      ];
      # Custom modules
      sentinel = {
        module = "sentinel";
        # Verified boot setup
        setupVerifiedBoot = true;
        # Disk encryption
        defaultEncryption = "luks2";
        encryptionOptions = [ "luks2" "plain" ];
        # Filesystem
        defaultFilesystem = "btrfs";
        filesystemOptions = [ "btrfs" "ext4" "xfs" "f2fs" ];
        # Security profiles
        securityProfiles = {
          standard = {
            kernel = "hardened";
            lockdown = "integrity";
            apparmor = true;
            ima = false;
          };
          hardened = {
            kernel = "hardened";
            lockdown = "confidentiality";
            apparmor = true;
            ima = true;
            evm = true;
            verifiedBoot = true;
            compartments = true;
          };
          paranoid = {
            kernel = "hardened-paranoid";
            lockdown = "confidentiality";
            apparmor = true;
            ima = true;
            evm = true;
            verifiedBoot = true;
            compartments = true;
            kasan = true;
            kcsan = true;
          };
        };
      };
    };
  };

  # Calamares branding
  systemd.services.calamares-branding = {
    description = "Setup Calamares Sentinel Branding";
    wantedBy = [ "multi-user.target" ];
    before = [ "calamares.service" ];
    script = ''
      mkdir -p /usr/share/calamares/branding/sentinel
      cat > /usr/share/calamares/branding/sentinel/branding.desc << 'EOF'
    componentName: sentinel
    version: "0.1.0"
    shortVersion: "0.1"
    welcomeImg: "welcome.png"
    logo: "logo.svg"
    applyColorScheme: true
    color:
      background: "#0a0a0f"
      foreground: "#e0e0e0"
      primary: "#00ff88"
      secondary: "#00cc6a"
      accent: "#ff3366"
    font:
      family: "JetBrains Mono"
      size: 12
    EOF

      # Copy logo and welcome image (would be provided)
      # cp /path/to/logo.svg /usr/share/calamares/branding/sentinel/
      # cp /path/to/welcome.png /usr/share/calamares/branding/sentinel/
    '';
  };

  # Installer-specific services
  systemd.services.sentinel-installer-prepare = {
    description = "Prepare Installer Environment";
    wantedBy = [ "multi-user.target" ];
    before = [ "calamares.service" ];
    script = ''
      # Generate hardware configuration
      ${pkgs.nixos-generate-config}/bin/nixos-generate-config --root /mnt --force

      # Detect hardware for kernel selection
      if ${pkgs.lscpu}/bin/lscpu | grep -q "GenuineIntel"; then
        echo "intel" > /run/sentinel/vendor
      elif ${pkgs.lscpu}/bin/lscpu | grep -q "AuthenticAMD"; then
        echo "amd" > /run/sentinel/vendor
      fi

      # Check TPM
      if ${pkgs.tpm2-tools}/bin/tpm2_getcap properties-fixed 2>/dev/null | grep -q "TPM_PT_FIXED"; then
        echo "tpm2" > /run/sentinel/tpm
      fi

      # Check for Intel TXT / AMD SKINIT
      if ${pkgs.lscpu}/bin/lscpu | grep -q "vmx\|svm"; then
        echo "virtualization" > /run/sentinel/virt
      fi

      # Check GPU
      if ${pkgs.lspci}/bin/lspci | grep -qi "nvidia"; then
        echo "nvidia" > /run/sentinel/gpu
      elif ${pkgs.lspci}/bin/lspci | grep -qi "amd.*vga\|ati.*vga"; then
        echo "amd" > /run/sentinel/gpu
      elif ${pkgs.lspci}/bin/lspci | grep -qi "intel.*vga\|intel.*display"; then
        echo "intel" > /run/sentinel/gpu
      fi
    '';
  };

  # Post-install setup
  systemd.services.sentinel-post-install = {
    description = "Sentinel Post-Install Configuration";
    wantedBy = [ "multi-user.target" ];
    after = [ "calamares.service" ];
    conditionPathExists = "/mnt/etc/nixos/configuration.nix";
    script = ''
      # This runs after Calamares installs the system
      # but before reboot

      # Copy verified boot keys
      mkdir -p /mnt/etc/sentinel/keys
      if [ -d /run/sentinel/keys ]; then
        cp -r /run/sentinel/keys/* /mnt/etc/sentinel/keys/
      fi

      # Set up TPM enrollment
      if [ -f /run/sentinel/tpm ]; then
        ${pkgs.sbctl}/bin/sbctl create-keys --root /mnt
        ${pkgs.sbctl}/bin/sbctl enroll-keys --root /mnt --force
      fi

      # Generate machine ID
      ${pkgs.systemd}/bin/systemd-machine-id-setup --root /mnt --print

      # Set up first-boot service
      mkdir -p /mnt/etc/systemd/system/sentinel-firstboot.service.d
      cat > /mnt/etc/systemd/system/sentinel-firstboot.service << 'SVC_EOF'
    [Unit]
    Description=Sentinel First Boot Setup
    After=network-online.target
    ConditionPathExists=!/etc/sentinel/.initialized

    [Service]
    Type=oneshot
    ExecStart=/usr/local/bin/sentinel-firstboot
    RemainAfterExit=yes

    [Install]
    WantedBy=multi-user.target
    SVC_EOF

      cat > /mnt/usr/local/bin/sentinel-firstboot << 'FB_EOF'
    #!/bin/sh
    set -eu

    echo "Running Sentinel first-boot setup..."

    # Enroll secure boot keys
    if [ -d /etc/sentinel/keys ]; then
      sbctl enroll-keys --force
    fi

    # Generate SSH host keys
    ssh-keygen -A

    # Set up TPM sealing
    if command -v tpm2_pcrread >/dev/null; then
      tpm2_pcrread -o /etc/sentinel/pcrs.json
    fi

    # Mark initialized
    touch /etc/sentinel/.initialized

    echo "First boot setup complete"
    FB_EOF
      chmod +x /mnt/usr/local/bin/sentinel-firstboot

      # Enable firstboot service
      ln -sf /etc/systemd/system/sentinel-firstboot.service /mnt/etc/systemd/system/multi-user.target.wants/

      # Clean up installer artifacts
      rm -f /mnt/etc/sudoers.d/calamares
      rm -f /mnt/etc/polkit-1/rules.d/49-calamares.rules
    '';
  };

  # NetworkManager for installer
  services.networkmanager = {
    enable = true;
    wifi.backend = "wpa_supplicant";
    dhcp = "internal";
    dispatchScripts = {
      "01-resolvconf" = ''
        #!/bin/sh
        if [ "$2" = "up" ]; then
          /usr/bin/resolvconf -a "$1" < /dev/null
        elif [ "$2" = "down" ]; then
          /usr/bin/resolvconf -d "$1"
        fi
      '';
    };
  };

  # Enable SSH for remote install
  services.openssh = {
    enable = true;
    settings = {
      PermitRootLogin = "prohibit-password";
      PasswordAuthentication = "no";
      PubkeyAuthentication = "yes";
      PermitEmptyPasswords = "no";
      ChallengeResponseAuthentication = "no";
      UsePAM = "yes";
      AuthenticationMethods = "publickey";
      # Allow installer user
      Match = "User sentinel";
      Match.Blocks = [
        "PasswordAuthentication yes"
        "PermitRootLogin no"
      ];
    };
  };

  # Set root password for installer (random, displayed on console)
  users.users.root.initialHashedPassword = "!";

  # Installer user with sudo
  users.users.sentinel = {
    isNormalUser = true;
    description = "Sentinel Installer";
    extraGroups = [ "wheel" "networkmanager" ];
    openssh.authorizedKeys.keys = [ ]; # Injected at boot
    shell = pkgs.bash;
  };

  # Sudo for installer user
  security.sudo.wheelNeedsPassword = false;

  # Auto-login for installer
  services.getty.autologinUser = "sentinel";

  # Disable swap on installer
  swapDevices = [ ];

  # Temporary files
  systemd.tmpfiles.rules = [
    "d /run/sentinel 0755 root root -"
    "d /run/sentinel/keys 0700 root root -"
  ];

  # Nix configuration for installer
  nix = {
    substituters = [ "https://cache.nixos.org" ];
    trustedPublicKeys = [ "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY=" ];
    extraOptions = ''
      experimental-features = nix-command flakes
      sandbox = false
    '';
  };

  # Documentation
  documentation.enable = false;
  man.enable = false;
}