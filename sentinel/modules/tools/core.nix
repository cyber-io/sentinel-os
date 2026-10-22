{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.sentinel.tools;
  profile = cfg.profile;
in
{
  options.sentinel.tools = {
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

  config = {
    # Base tools for all profiles
    environment.systemPackages = with pkgs; [
      # Essential
      bash
      coreutils
      findutils
      grep
      gnugrep
      gawk
      sed
      less
      vim
      git
      # Network basics
      iproute2
      net-tools
      nmap-ncat
      socat
      tcpdump
      wireshark-cli
      # Crypto
      gnupg
      age
      sops
      openssl
      # System
      htop
      btop
      lsof
      strace
      perf
      sysstat
      # Compression
      gzip
      bzip2
      xz
      zstd
      tar
      unzip
      p7zip
      # File analysis
      file
      binutils
      xxd
      hexdump
      # Process
      procps
      psmisc
      # Hardware
      lm_sensors
      smartmontools
      dmidecode
      hwloc
      lsusb
      lspci
      # TPM
      tpm2-tools
      tpm2-tss
      # Nix
      nix
      nixpkgs-fmt
      statix
    ];

    # Profile-specific tools
  } // lib.optionalAttrs (profile == "minimal") {
    environment.systemPackages = with pkgs; [
      # Minimal recon
      nmap
      masscan
      rustscan
      # Basic web
      curl
      wget
      httpie
      # DNS
      bind-utils
      drill
      # SSL
      testssl.sh
      sslscan
    ];
  } // lib.optionalAttrs (profile == "core") {
    environment.systemPackages = with pkgs; [
      # Recon & Discovery
      nmap
      masscan
      rustscan
      zmap
      zgrab2
      amass
      subfinder
      dnsx
      httpx
      nuclei
      naabu
      mapcidr
      # Web Application
      curl
      wget
      httpie
      ffuf
      feroxbuster
      gobuster
      dirsearch
      wafw00f
      whatweb
      # SSL/TLS
      testssl.sh
      sslscan
      sslyze
      # Network
      netcat
      socat
      nmap-ncat
      proxychains-ng
      sshuttle
      # Passwords
      hashcat
      john
      hydra
      medusa
      # Wireless (basic)
      aircrack-ng
      # Exploitation
      metasploit
      # Post-exploitation
      bloodhound
      sharphound
      # Forensics (basic)
      volatility3
      autopsy
      sleuthkit
      # Reverse Engineering (basic)
      ghidra
      radare2
      # Container/Cloud
      dive
      kube-hunter
      kube-bench
      # Reporting
      pandoc
      weasyprint
    ];
  } // lib.optionalAttrs (profile == "full") {
    environment.systemPackages = with pkgs; [
      # Extended recon
      nmap
      masscan
      rustscan
      zmap
      zgrab2
      amass
      subfinder
      dnsx
      httpx
      nuclei
      naabu
      mapcidr
      assetfinder
      findomain
      shuffledns
      puredns
      alterx
      gotator
      # Web Application
      curl
      wget
      httpie
      ffuf
      feroxbuster
      gobuster
      dirsearch
      wafw00f
      whatweb
      nikto
      skipfish
      arachni
      sqlmap
      nosqlmap
      xsstrike
      commix
      # API
      postman
      insomnia
      swagger-codegen
      # SSL/TLS
      testssl.sh
      sslscan
      sslyze
      tlsenum
      # Network
      netcat
      socat
      nmap-ncat
      proxychains-ng
      sshuttle
      iodine
      dnscat2
      # Passwords
      hashcat
      john
      hydra
      medusa
      patator
      crowbar
      # Wireless
      aircrack-ng
      kismet
      wifite
      eaphammer
      hostapd-wpe
      # Bluetooth
      bluez
      bluez-tools
      ubertooth
      # RFID/NFC
      libnfc
      mfoc
      mfcuk
      proxmark3
      # Exploitation
      metasploit
      sliver
      cobaltstrike # Requires license
      # C2
      mythic
      havoc
      # Post-exploitation
      bloodhound
      sharphound
      certipy
      adfind
      pingcastle
      # Lateral movement
      crackmapexec
      impacket
      evil-winrm
      remmina
      xfreerdp
      # Persistence
      sharppersist
      # Privilege escalation
      linpeas
      winpeas
      pspy
      # Forensics
      volatility3
      autopsy
      sleuthkit
      bulk-extractor
      foremost
      scalpel
      photorec
      testdisk
      # Memory
      lime
      avml
      # Disk
      ddrescue
      dc3dd
      ewf-tools
      afflib
      # Network forensics
      zeek
      suricata
      moloch
      ntopng
      # Malware analysis
      ghidra
      radare2
      ida-free
      cutter
      yara
      yaragen
      capa
      floss
      strings
      peframe
      manalyze
      # Sandbox
      cape
      cuckoo
      # Reverse Engineering
      binaryninja # Requires license
      hopper # Requires license
      # Decompilation
      retdec
      snowman
      # Debugging
      gdb
      gef
      pwndbg
      radare2
      lldb
      # Fuzzing
      aflplusplus
      honggfuzz
      libfuzzer
      # Container/Cloud
      dive
      kube-hunter
      kube-bench
      kubeaudit
      kubescape
      trivy
      grype
      syft
      cosign
      crane
      oras
      # AWS
      awscli
      prowler
      scoutsuite
      pacu
      # Azure
      azure-cli
      microsoft-graph
      # GCP
      gcloud
      scoutsuite
      # IaC
      terraform
      terratest
      checkov
      tfsec
      # CI/CD
      gitlab-runner
      drone
      tekton
      # Reporting
      pandoc
      weasyprint
      jupyter
      jupyterlab
      # Documentation
      mkdocs
      mkdocs-material
      # Visualization
      graphviz
      mermaid-cli
      plantuml
    ];
  } // lib.optionalAttrs (profile == "forensics") {
    environment.systemPackages = with pkgs; [
      # Core forensics
      volatility3
      autopsy
      sleuthkit
      bulk-extractor
      foremost
      scalpel
      photorec
      testdisk
      # Memory
      lime
      avml
      winpmem
      osxpmem
      # Disk
      ddrescue
      dc3dd
      ewf-tools
      afflib
      aff4
      # Filesystem
      extundelete
      debugfs
      # Network
      zeek
      suricata
      moloch
      ntopng
      wireshark
      tshark
      # Malware
      ghidra
      radare2
      ida-free
      cutter
      yara
      yaragen
      capa
      floss
      strings
      peframe
      manalyze
      # Registry
      regripper
      # Event logs
      evtx
      evtx_dump
      # Timeline
      plaso
      log2timeline
      timesketch
      # Mobile
      adb
      fastboot
      ideviceinstaller
      libimobiledevice
      # Cloud
      awscli
      azure-cli
      gcloud
    ];
  } // lib.optionalAttrs (profile == "wireless") {
    environment.systemPackages = with pkgs; [
      # WiFi
      aircrack-ng
      kismet
      wifite
      eaphammer
      hostapd-wpe
      wpa_supplicant
      iw
      wireless-tools
      # Bluetooth
      bluez
      bluez-tools
      ubertooth
      btlejack
      gattacker
      # RFID/NFC
      libnfc
      mfoc
      mfcuk
      proxmark3
      chameleonmini
      # SDR
      hackrf
      rtl-sdr
      bladeRF
      limesdr
      gqrx
      cubicsdr
      inspectrum
      urh
      # GPS
      gpsd
      gpsbabel
      # Antennas
      kalibrate-rtl
      rtl-power
    ];
  } // lib.optionalAttrs (profile == "cloud") {
    environment.systemPackages = with pkgs; [
      # Container
      dive
      kube-hunter
      kube-bench
      kubeaudit
      kubescape
      trivy
      grype
      syft
      cosign
      crane
      oras
      crictl
      ctr
      nerdctl
      # AWS
      awscli
      prowler
      scoutsuite
      pacu
      cloudsploit
      # Azure
      azure-cli
      microsoft-graph
      stormspotter
      # GCP
      gcloud
      scoutsuite
      gcp-audit
      # Kubernetes
      kubectl
      helm
      kustomize
      k9s
      stern
      kubectx
      kubens
      # IaC
      terraform
      terratest
      checkov
      tfsec
      terrascan
      # CI/CD
      gitlab-runner
      drone
      tekton
      argo
      # Service Mesh
      istioctl
      linkerd
      # Monitoring
      prometheus
      grafana
      loki
      tempo
      # Logging
      fluentd
      fluent-bit
      vector
    ];
  };

  # Custom additional tools
  environment.systemPackages = with pkgs; cfg.custom;
}