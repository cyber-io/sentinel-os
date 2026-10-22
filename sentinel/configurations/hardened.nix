{ config, lib, pkgs, ... }:

with lib;

{
  # Production hardened configuration
  # Maximum security, minimal attack surface

  # Import hardened kernel
  imports = [ ./../modules/kernel/hardened.nix ];

  # Verified boot
  imports = [ ./../modules/verified-boot.nix ];

  # Compartments
  imports = [ ./../modules/compartments.nix ];

  # AI inference
  imports = [ ./../modules/ai/inference.nix ];

  # Full toolset
  imports = [ ./../modules/tools/all.nix ];

  # ============ SECURITY SETTINGS ============

  # Kernel lockdown
  security.lockKernelModules = true;

  # AppArmor
  security.apparmor.enable = true;
  security.apparmor.profiles = {
    # Additional production profiles
  };

  # IMA/EVM
  security.ima.enable = true;
  security.evm.enable = true;

  # Audit
  security.audit.enable = true;

  # Systemd hardening (already in base.nix, reinforce)
  systemd = {
    protectKernelTunables = true;
    protectKernelModules = true;
    protectControlGroups = true;
    restrictNamespaces = true;
    lockPersonality = true;
    memoryDenyWriteExecute = true;
    restrictAddressFamilies = [ "AF_UNIX" "AF_INET" "AF_INET6" ];
    restrictRealtime = true;
    systemCallFilter = [ "@system-service" ];
    systemCallFilterMask = [ "@obsolete" "@debug" "@module" "@raw-io" "@reboot" "@swap" ];
    privateTmp = true;
    privateDevices = true;
    protectHome = true;
    protectSystem = "strict";
    readOnlyPaths = [ "/etc" "/usr" "/boot" "/nix/store" ];
    inaccessiblePaths = [ "/root" "/home/*/.ssh" "/var/lib/sentinel/keys" ];
  };

  # No direct root login
  users.users.root = {
    hashedPassword = "!";
    unlocked = false;
  };

  # Sudo with strict logging
  security.sudo.configFile = ''
    Defaults log_output, log_input
    Defaults logfile="/var/log/sudo.log"
    Defaults requiretty
    Defaults !authenticate
    Defaults timestamp_timeout=0
    %wheel ALL=(ALL) ALL
  '';

  # Firewall - default deny
  networking.firewall = {
    enable = true;
    allowPing = false;
    logRejectedPackets = true;
    logRejectedConnections = true;
    # Only allow management VLAN
    allowedTCPPorts = [ ];
    allowedUDPPorts = [ ];
    # Custom rules in compartments.nix
  };

  # Network hardening
  networking = {
    hostName = "sentinel";
    disableIPv6 = false;
    # Disable unused protocols
    kernelModules = [ "tcp_cubic" "tcp_bbr" ];
    # Sysctl in kernel/hardened.nix
  };

  # SSH - key only, no passwords, restricted
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
      MaxSessions = 2;
      AllowUsers = "sentinel";
      AllowGroups = "wheel";
      Banner = "/etc/ssh/banner";
      # Port knocking
      # Port = 2222;
    };
  };

  # Fail2ban for SSH
  services.fail2ban = {
    enable = true;
    jails = {
      sshd = {
        enabled = true;
        port = "ssh";
        filter = "sshd";
        logpath = "/var/log/auth.log";
        maxretry = 3;
        bantime = 3600;
        findtime = 600;
      };
    };
  };

  # ============ STORAGE ============

  # Encrypted root with Btrfs
  # (Configured via hardware-configuration.nix at install time)

  # Swap encryption
  swapDevices = [ {
    device = "/dev/disk/by-label/swap";
    randomEncryption = true;
  } ];

  # ============ BOOT ============

  # systemd-boot with secure boot
  boot.loader = {
    efi = {
      canTouchEfiVariables = true;
      efiSysMountPoint = "/boot/efi";
    };
    systemd-boot = {
      enable = true;
      secureBoot = true;
      # Kernel command line hardening
      extraEntries = [ {
        title = "Sentinel OS (Recovery)";
        linux = "/boot/vmlinuz-linux-hardened";
        initrd = "/boot/initramfs-linux-hardened.img";
        options = "systemd.unit=emergency.target";
      } ];
    };
  };

  # Kernel command line (additional to hardened.nix)
  boot.kernelParams = [
    # Module loading
    "module.sig_enforce=1"
    "module_blacklist=firewire-core,firewire-ohci,firewire-sbp2,thunderbolt,bluetooth"
    # Lockdown
    "lockdown=confidentiality"
    # KPTI
    "kpti=on"
    # Mitigations
    "spectre_v2=on"
    "spec_store_bypass_disable=on"
    "tsx=off"
    "tsx_async_abort=full,nosmt"
    "mds=full,nosmt"
    "l1tf=full,force"
    "mmio_stale_data=on"
    "srbds=on"
    # Randomization
    "randomize_kstack_offset=on"
    "kaslr"
    # Disable dangerous features
    "sysrq_always_enabled=0"
    "kexec_load_disabled=1"
    "module.sig_hash=sha512"
    # IOMMU
    "intel_iommu=on,strict"
    "amd_iommu=on,strict"
    # TPM
    "tpm_tis.interrupts=1"
    "tpm_tis.force=1"
    # AppArmor
    "apparmor=1"
    "security=apparmor"
    # Audit
    "audit=1"
    "audit_backlog_limit=8192"
  ];

  # ============ USERS ============

  # Sentinel operator user
  users.users.sentinel = {
    isNormalUser = true;
    description = "Sentinel Operator";
    extraGroups = [ "wheel" "kvm" "render" "video" "sentinel-ai" ];
    # No password - SSH key only
    openssh.authorizedKeys.keys = [ ]; # Populated at deploy
    shell = pkgs.zsh;
  };

  # AI service user
  users.users.sentinel-ai = {
    isSystemUser = true;
    description = "Sentinel AI Inference";
    group = "sentinel-ai";
    home = "/var/lib/sentinel/ai";
  };

  users.groups.sentinel-ai = {
    gid = 990;
    members = [ "sentinel" ];
  };

  # ============ SERVICES ============

  # Time synchronization (chrony for better security)
  services.chrony = {
    enable = true;
    servers = [
      "0.pool.ntp.org"
      "1.pool.ntp.org"
      "2.pool.ntp.org"
      "3.pool.ntp.org"
    ];
    # Disable NTP control
    cmdport = 0;
  };

  # Disable timesyncd (using chrony)
  services.timesyncd.enable = false;

  # Logging
  services.journald.extraConfig = {
    Storage = "persistent";
    Compress = "yes";
    ForwardToSyslog = "no";
    ForwardToKMsg = "no";
    ForwardToConsole = "no";
    MaxLevelStore = "info";
    MaxLevelSyslog = "warning";
    MaxLevelKMsg = "notice";
    MaxLevelConsole = "emerg";
    SystemMaxUse = "2G";
    RuntimeMaxUse = "500M";
    MaxFileSec = "1month";
    MaxRetentionSec = "1year";
    SyncIntervalSec = "5min";
  };

  # Auditd
  services.auditd = {
    enable = true;
    rules = ''
      # System calls
      -a always,exit -F arch=b64 -S execve -k exec
      -a always,exit -F arch=b64 -S openat -k file_open
      -a always,exit -F arch=b64 -S connect -k network
      -a always,exit -F arch=b64 -S bind -k network
      -a always,exit -F arch=b64 -S capset -k caps
      # File watches
      -w /etc -p wa -k etc_changes
      -w /boot -p wa -k boot_changes
      -w /nix/store -p wa -k store_changes
      -w /var/lib/sentinel -p wa -k sentinel_changes
      -w /etc/sentinel -p wa -k sentinel_config
      # Identity
      -w /etc/passwd -p wa -k identity
      -w /etc/group -p wa -k identity
      -w /etc/shadow -p wa -k identity
      -w /etc/sudoers -p wa -k identity
      # Network
      -w /etc/hosts -p wa -k network
      -w /etc/resolv.conf -p wa -k network
    '';
  };

  # Logrotate
  services.logrotate = {
    enable = true;
    config = {
      "/var/log/sudo.log" = {
        daily = true;
        rotate = 30;
        compress = true;
        delaycompress = true;
        missingok = true;
        notifempty = true;
        create = "0600 root root";
      };
      "/var/log/audit/audit.log" = {
        daily = true;
        rotate = 90;
        compress = true;
        delaycompress = true;
        missingok = true;
        notifempty = true;
        create = "0600 root root";
        sharedscripts = true;
        postrotate = "/usr/bin/systemctl reload auditd > /dev/null 2>&1 || true";
      };
    };
  };

  # ============ HARDWARE ============

  # TPM
  hardware.tpm2 = {
    enable = true;
    tcsd.enable = true;
  };

  # CPU microcode
  hardware.cpu = {
    intel = {
      updateMicrocode = true;
      manageMicrocode = true;
    };
    amd = {
      updateMicrocode = true;
    };
  };

  # Entropy
  services.haveged = {
    enable = true;
    config = { "write-wakeup-threshold" = 1024; };
  };

  # Hardware monitoring
  services.lm-sensors.enable = true;

  # Smartmontools
  services.smartd = {
    enable = true;
    options = "-q onecheck -m sentinel@localhost -M exec /usr/share/smartmontools/smartd-runner";
  };

  # ============ NIX ============

  nix = {
    # Binary cache
    substituters = [
      "https://cache.nixos.org"
      "https://sentinel-cache.example.org"
    ];
    trustedPublicKeys = [
      "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
      "sentinel-cache.example.org:..."
    ];
    # Sandbox
    sandbox = true;
    sandboxPaths = [ "/var/lib/sentinel" ];
    # Experimental
    extraOptions = ''
      experimental-features = nix-command flakes pipe-operators
      auto-optimise-store = true
      max-jobs = auto
      cores = 0
      keep-going = true
      keep-failed = false
    '';
    # GC
    gc = {
      automatic = true;
      dates = "03:00";
      options = "--delete-older-than 7d";
    };
  };

  # ============ MAINTENANCE ============

  # Auto-updates (security only)
  systemd.services.sentinel-security-updates = {
    description = "Sentinel Security Updates";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${pkgs.writeScriptBin "sentinel-security-updates" ''}
        #!${pkgs.runtimeShell}
        # Check for security updates
        ${pkgs.nix}/bin/nix flake update --flake /etc/nixos --commit-lock-file
        # Rebuild if changes
        if ! git -C /etc/nixos diff --quiet; then
          ${pkgs.nixos-rebuild}/bin/nixos-rebuild switch --flake /etc/nixos#hardened
          # Notify
          echo "Security updates applied" | ${pkgs.mailutils}/bin/mail -s "Sentinel: Security Updates" root
        fi
      '';
      User = "root";
    };
  };

  systemd.timers.sentinel-security-updates = {
    description = "Daily Security Update Check";
    wantedBy = [ "timers.target" ];
    partOf = [ "sentinel-security-updates.service" ];
    timerConfig = {
      OnCalendar = "daily";
      RandomizedDelaySec = "2h";
      Persistent = true;
    };
  };

  # Health check
  systemd.services.sentinel-health-check = {
    description = "Sentinel Health Check";
    wantedBy = [ "multi-user.target" ];
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${pkgs.writeScriptBin "sentinel-health-check" ''}
        #!${pkgs.runtimeShell}
        set -euo pipefail

        FAILURES=0

        # Check TPM
        if ! ${pkgs.tpm2-tools}/bin/tpm2_pcrread >/dev/null 2>&1; then
          echo "FAIL: TPM not accessible"
          FAILURES=$((FAILURES + 1))
        fi

        # Check kernel lockdown
        if [ "$(cat /sys/kernel/security/lockdown)" != "confidentiality" ]; then
          echo "FAIL: Kernel not in confidentiality lockdown"
          FAILURES=$((FAILURES + 1))
        fi

        # Check AppArmor
        if ! ${pkgs.apparmor}/bin/aa-status --enabled >/dev/null 2>&1; then
          echo "FAIL: AppArmor not enabled"
          FAILURES=$((FAILURES + 1))
        fi

        # Check IMA
        if [ ! -f /sys/kernel/security/ima/ascii_runtime_measurements ]; then
          echo "FAIL: IMA not measuring"
          FAILURES=$((FAILURES + 1))
        fi

        # Check AI services
        for svc in sentinel-llamacpp sentinel-vllm sentinel-ai-gateway; do
          if ! systemctl is-active --quiet "$svc"; then
            echo "FAIL: $svc not running"
            FAILURES=$((FAILURES + 1))
          fi
        done

        # Check compartment runtime
        if ! systemctl is-active --quiet sentinel-crosvm; then
          echo "FAIL: Compartment runtime not running"
          FAILURES=$((FAILURES + 1))
        fi

        # Check capability broker
        if [ ! -S /run/sentinel/capability-broker.sock ]; then
          echo "FAIL: Capability broker socket not found"
          FAILURES=$((FAILURES + 1))
        fi

        # Check disk space
        DISK_USE=$(${pkgs.coreutils}/bin/df / | ${pkgs.gawk}/bin/awk 'NR==2 {print $5}' | ${pkgs.coreutils}/bin/sed 's/%//')
        if [ "$DISK_USE" -gt 90 ]; then
          echo "FAIL: Disk usage > 90% ($DISK_USE%)"
          FAILURES=$((FAILURES + 1))
        fi

        # Check memory
        MEM_USE=$(${pkgs.coreutils}/bin/free | ${pkgs.gawk}/bin/awk 'NR==2 {printf "%.0f", $3/$2*100}')
        if [ "$MEM_USE" -gt 95 ]; then
          echo "FAIL: Memory usage > 95% ($MEM_USE%)"
          FAILURES=$((FAILURES + 1))
        fi

        if [ $FAILURES -gt 0 ]; then
          echo "HEALTH CHECK FAILED: $FAILURES issues"
          exit 1
        else
          echo "HEALTH CHECK PASSED"
        fi
      '';
      User = "root";
    };
  };

  systemd.timers.sentinel-health-check = {
    description = "Sentinel Health Check (every 5 minutes)";
    wantedBy = [ "timers.target" ];
    partOf = [ "sentinel-health-check.service" ];
    timerConfig = {
      OnCalendar = "*:0/5";
      Persistent = true;
    };
  };

  # ============ MONITORING ============

  # Prometheus node exporter
  services.prometheus-node-exporter = {
    enable = true;
    port = 9100;
    extraFlags = [
      "--collector.systemd"
      "--collector.processes"
      "--collector.filesystem.ignored-mount-points=^/(sys|proc|dev|run|var/lib/sentinel/compartments)($|/)"
      "--collector.netdev.ignored-devices=^veth.*$"
    ];
  };

  # Vector for log aggregation
  services.vector = {
    enable = true;
    config = ''
      [sources.journald]
      type = "journald"

      [sources.audit]
      type = "file"
      include = [ "/var/log/audit/audit.log" ]

      [transforms.parse_audit]
      type = "remap"
      inputs = [ "audit" ]
      source = '.message = parse_regex!(.message, r''^(?P<timestamp>\S+)\s+(?P<host>\S+)\s+(?P<message>.*)$'')'

      [sinks.console]
      type = "console"
      inputs = [ "journald", "parse_audit" ]
      encoding = "json"
    '';
  };

  # ============ AI/ML ============

  # Sentinel AI configuration
  sentinel.ai = {
    enable = true;
    gpuSupport = true;
    modelsPath = "/var/lib/sentinel/models";
    registry.enable = true;
    inference = {
      llamacpp.enable = true;
      vllm.enable = true;
      gateway.enable = true;
    };
    monitoring.enable = true;
  };

  # ============ COMPARTMENTS ============

  sentinel.compartments = {
    enable = true;
    backend = "crosvm";
    network.enable = true;
    gpu.enable = true;
  };

  # ============ VERIFIED BOOT ============

  sentinel.verifiedBoot = {
    enable = true;
    heads.enable = false; # Flashed separately
    trenchboot.enable = true;
    keys.dir = "/etc/sentinel/keys";
  };

  # ============ CLEANUP ============

  # Remove unnecessary packages
  environment.systemPackages = lib.filter (p: p.name != "nano") config.environment.systemPackages;

  # Documentation
  documentation.enable = false;
  man.enable = false;
}