{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.sentinel;
in
{
  options.sentinel.verifiedBoot = {
    enable = mkEnableOption "Verified boot chain (Heads + TrenchBoot)";
    # Heads firmware configuration
    heads = {
      enable = mkEnableOption "Heads firmware payload";
      config = mkOption {
        type = types.attrs;
        default = {
          # Boot menu timeout
          timeout = 5;
          # Default boot entry
          default = "Sentinel OS (Hardened)";
          # TPM settings
          tpm = {
            enable = true;
            pcr_bank = "sha256";
          };
          # GPG keys for verification
          gpg = {
            enable = true;
            keys = [ ]; # Populated at build time
          };
          # Kernel command line
          kernel_cmdline = "module.sig_enforce=1 lockdown=confidentiality";
          # Measured boot
          measured_boot = true;
          # Anti-evil maid
          anti_evil_maid = true;
        };
        description = "Heads configuration";
      };
    };

    # TrenchBoot D-RTM
    trenchboot = {
      enable = mkEnableOption "TrenchBoot Dynamic Root of Trust";
      acm = mkOption {
        type = types.path;
        default = null;
        description = "Path to Intel TXT ACM";
      };
      kernel = mkOption {
        type = types.path;
        description = "Kernel to measure";
      };
      initrd = mkOption {
        type = types.path;
        description = "Initrd to measure";
      };
      policy = mkOption {
        type = types.path;
        default = null;
        description = "Launch control policy";
      };
    };

    # Key management
    keys = {
      # Path to key directory
      dir = mkOption {
        type = types.path;
        default = "/etc/sentinel/keys";
        description = "Directory containing signing keys";
      };
      # Kernel signing key
      kernelSigningKey = mkOption {
        type = types.path;
        default = "/etc/sentinel/keys/kernel-signing.key";
      };
      kernelSigningCert = mkOption {
        type = types.path;
        default = "/etc/sentinel/keys/kernel-signing.crt";
      };
      # Initrd signing key
      initrdSigningKey = mkOption {
        type = types.path;
        default = "/etc/sentinel/keys/initrd-signing.key";
      };
      initrdSigningCert = mkOption {
        type = types.path;
        default = "/etc/sentinel/keys/initrd-signing.crt";
      };
      # Module signing key
      moduleSigningKey = mkOption {
        type = types.path;
        default = "/etc/sentinel/keys/module-signing.key";
      };
      moduleSigningCert = mkOption {
        type = types.path;
        default = "/etc/sentinel/keys/module-signing.crt";
      };
    };
  };

  config = mkIf cfg.verifiedBoot.enable {
    # TPM 2.0 support
    hardware.tpm2 = {
      enable = true;
      tcsd.enable = true;
    };

    # Kernel signing
    boot.kernel.sign = true;
    boot.kernel.signingKey = cfg.verifiedBoot.keys.kernelSigningKey;
    boot.kernel.signingCert = cfg.verifiedBoot.keys.kernelSigningCert;

    # Module signing
    boot.kernelModules = [ "kvm" "kvm_intel" "kvm_amd" ];
    # Note: Module signing keys configured in kernel config

    # Initrd signing
    boot.initrd.sign = true;
    boot.initrd.signingKey = cfg.verifiedBoot.keys.initrdSigningKey;
    boot.initrd.signingCert = cfg.verifiedBoot.keys.initrdSigningCert;

    # Secure boot
    boot.loader.efi = {
      canTouchEfiVariables = true;
      efiSysMountPoint = "/boot/efi";
    };

    # systemd-boot with secure boot
    boot.loader.systemd-boot = {
      enable = true;
      secureBoot = true;
      # Keys enrolled via sbctl
    };

    # sbctl for secure boot key management
    environment.systemPackages = with pkgs; [ sbctl ];

    # sbctl configuration
    systemd.services.sbctl-setup = {
      description = "Enroll secure boot keys";
      wantedBy = [ "multi-user.target" ];
      after = [ "systemd-udev-settle.service" ];
      conditionPathExists = "/boot/efi";
      script = ''
        ${pkgs.sbctl}/bin/sbctl create-keys || true
        ${pkgs.sbctl}/bin/sbctl enroll-keys --force || true
        ${pkgs.sbctl}/bin/sbctl sign -s ${config.boot.kernelPackages.kernel}/Image
        ${pkgs.sbctl}/bin/sbctl sign -s ${config.boot.kernelPackages.kernel}/Image.signed
      '';
    };

    # Heads firmware (if enabled)
    # Note: Heads is typically flashed to SPI, not installed via NixOS
    # This creates the configuration for building Heads
    mkIf cfg.verifiedBoot.heads.enable {
      environment.systemPackages = with pkgs; [ heads ];

      # Heads build configuration
      systemd.services.heads-config = {
        description = "Generate Heads configuration";
        wantedBy = [ "multi-user.target" ];
        script = ''
          mkdir -p /etc/heads
          cat > /etc/heads/config.json << 'EOF'
          ${builtins.toJSON cfg.verifiedBoot.heads.config}
          EOF
        '';
      };
    };

    # TrenchBoot D-RTM (if enabled)
    mkIf cfg.verifiedBoot.trenchboot.enable {
      # Requires Intel TXT capable hardware
      environment.systemPackages = with pkgs; [ trenchboot ];

      # Create launch control policy
      systemd.services.trenchboot-setup = {
        description = "Configure TrenchBoot D-RTM";
        wantedBy = [ "multi-user.target" ];
        requires = [ "systemd-udev-settle.service" ];
        after = [ "systemd-udev-settle.service" ];
        script = ''
          mkdir -p /etc/trenchboot
          # Copy ACM
          if [ -f "${cfg.verifiedBoot.trenchboot.acm}" ]; then
            cp "${cfg.verifiedBoot.trenchboot.acm}" /boot/acm.bin
          fi
          # Copy policy if provided
          if [ -f "${cfg.verifiedBoot.trenchboot.policy}" ]; then
            cp "${cfg.verifiedBoot.trenchboot.policy}" /etc/trenchboot/policy
          fi
          # Generate measured kernel/initrd entries
          ${pkgs.trenchboot}/bin/tb_polgen \
            --kernel "${cfg.verifiedBoot.trenchboot.kernel}" \
            --initrd "${cfg.verifiedBoot.trenchboot.initrd}" \
            --output /boot/measurements.bin \
            --pcr 16
        '';
      };

      # Kernel command line for TrenchBoot
      boot.kernelParams = [ "trusted_boot=tb" "tpm_tis.interrupts=1" ];
    };

    # IMA/EVM for runtime integrity measurement
    security.ima = {
      enable = true;
      policy = ''
        # Measure all executables
        measure func=BPRM_CHECK mask=MAY_EXEC
        # Measure all file opens for read
        measure func=FILE_CHECK mask=MAY_READ
        # Measure memory mappings
        measure func=MMAP_CHECK mask=MAY_EXEC
        # Appraise all executables
        appraise func=BPRM_CHECK mask=MAY_EXEC
        # Appraise all modules
        appraise func=MODULE_CHECK
        # Appraise firmware
        appraise func=FIRMWARE_CHECK
        # Appraise policy
        appraise func=POLICY_CHECK
        # Ignore /proc, /sys, /dev
        dont_measure fsmagic=0x9fa0
        dont_measure fsmagic=0x62656572
        dont_measure fsmagic=0x64626720
      '';
      hash = "sha256";
      template = "ima-ng";
      keyrings = "permit_signed_by_builtin_or_secondary";
    };

    # EVM (Extended Verification Module)
    security.evm = {
      enable = true;
      hmac = true;
      key = "/etc/keys/evm-key";
    };

    # TPM event log
    systemd.services.tpm2-eventlog = {
      description = "TPM2 Event Log Service";
      wantedBy = [ "multi-user.target" ];
      after = [ "tpm2-abrmd.service" ];
      serviceConfig = {
        ExecStart = "${pkgs.tpm2-tools}/bin/tpm2_eventlog /sys/kernel/security/tpm0/binary_bios_measurements";
        Restart = "on-failure";
      };
    };

    # Verified boot attestation service
    systemd.services.sentinel-attestation = {
      description = "Sentinel Attestation Service";
      wantedBy = [ "multi-user.target" ];
      after = [ "network-online.target" "tpm2-abrmd.service" ];
      serviceConfig = {
        ExecStart = "${pkgs.writeScriptBin "sentinel-attestation" ''}
          #!${pkgs.runtimeShell}
          ${pkgs.tpm2-tools}/bin/tpm2_pcrread -o /run/sentinel/pcrs.json
          ${pkgs.jq}/bin/jq '.[] | select(.pcr == 0 or .pcr == 1 or .pcr == 2 or .pcr == 3 or .pcr == 4 or .pcr == 5 or .pcr == 6 or .pcr == 7)' /run/sentinel/pcrs.json > /run/sentinel/rtm-pcrs.json
          ${pkgs.jq}/bin/jq '.[] | select(.pcr >= 8 and .pcr <= 15)' /run/sentinel/pcrs.json > /run/sentinel/drtm-pcrs.json
          ${pkgs.jq}/bin/jq '.[] | select(.pcr >= 16)' /run/sentinel/pcrs.json > /run/sentinel/os-pcrs.json
          # Sign PCRs with attestation key
          ${pkgs.openssl}/bin/openssl dgst -sha256 -sign /etc/sentinel/keys/attestation.key -out /run/sentinel/pcrs.sig /run/sentinel/pcrs.json
        '';
        User = "sentinel";
        Group = "sentinel";
        StateDirectory = "sentinel";
      };
    };

    # PCR policy for sealed secrets
    systemd.services.sentinel-pcr-policy = {
      description = "Sentinel PCR Policy Setup";
      wantedBy = [ "multi-user.target" ];
      before = [ "sentinel-attestation.service" ];
      script = ''
        mkdir -p /etc/sentinel/policies
        # Create policy for sealing secrets to known-good PCRs
        cat > /etc/sentinel/policies/unseal.pol << 'EOF'
        {
          "description": "Sentinel OS unseal policy",
          "pcrs": {
            "0": "RTM (CRTM, BIOS, Option ROMs)",
            "1": "Platform Configuration",
            "2": "Option ROM Code",
            "3": "Option ROM Config",
            "4": "Bootloader (Heads)",
            "5": "Bootloader Config",
            "6": "ACPI Tables",
            "7": "Secure Boot State",
            "8": "D-RTM Launch (TrenchBoot ACM)",
            "9": "D-RTM Kernel",
            "10": "D-RTM Initrd",
            "11": "D-RTM Policy",
            "12": "Reserved",
            "13": "Reserved",
            "14": "Reserved",
            "15": "Debug",
            "16": "Initrd Hash",
            "17": "Kernel Hash",
            "18": "Kernel Command Line",
            "19": "seL4 Hash",
            "20": "Root FS Hash",
            "21": "System Config Hash",
            "22": "AppArmor Policy Hash",
            "23": "Application Hashes"
          }
        }
        EOF
      '';
    };
  };
}