{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.sentinel.tools;
in
{
  # This module includes ALL tools from all profiles
  # Use with caution - very large system closure

  environment.systemPackages = with pkgs; [
    # ============ RECON & DISCOVERY ============
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
    # Additional
    dnsrecon
    dnsenum
    fierce
    gobuster
    dirb
    dirsearch
    feroxbuster
    ffuf
    wafw00f
    whatweb
    nikto
    skipfish
    arachni

    # ============ WEB APPLICATION ============
    curl
    wget
    httpie
    sqlmap
    nosqlmap
    xsstrike
    commix
    postman
    insomnia
    swagger-codegen
    graphql-voyager
    postgraphile

    # ============ SSL/TLS ============
    testssl.sh
    sslscan
    sslyze
    tlsenum
    tlsfingerprint

    # ============ NETWORK ============
    netcat
    socat
    nmap-ncat
    proxychains-ng
    sshuttle
    iodine
    dnscat2
    chisel
    ligolo-ng
    # VPN
    openvpn
    wireguard-tools
    openconnect
    # Tunnels
    ngrok
    cloudflared

    # ============ PASSWORD ATTACKS ============
    hashcat
    john
    hydra
    medusa
    patator
    crowbar
    # Wordlists
    seclists
    rockyou
    wordlists
    # Rule files
    hashcat-rules
    john-rules

    # ============ WIRELESS ============
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

    # ============ EXPLOITATION ============
    metasploit
    sliver
    # cobaltstrike (requires license)
    # Core frameworks
    exploitdb
    searchsploit
    # Shellcode
    msfvenom
    donut
    sct
    # Payload generators
    veil
    shelly
    # AV evasion
    hyperion
    pezor
    # Exploit dev
    pwntools
    ropper
    ropgadget
    one-gadget
    libc-database

    # ============ C2 FRAMEWORKS ============
    mythic
    havoc
    # sliver (already listed)
    # cobaltstrike (license)
    # Empire
    starkiller
    # Covenant

    # ============ POST-EXPLOITATION ============
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
    linux-exploit-suggester
    windows-exploit-suggester
    # Credential access
    mimikatz
    sekurlsa
    lsassy
    donpapi
    # LSASS dumping
    comsvcs.dll
    procdump
    # Kerberos
    kerbrute
    rubeus
    kekeo
    # DPAPI
    dpapi
    # Token manipulation
    tokenvator
    # WMI
    wmiexec
    wmic
    # SMB
    smbclient
    smbmap
    enum4linux
    enum4linux-ng
    # RPC
    rpcdump
    rpcmap
    # LDAP
    ldapsearch
    ldapenum
    windapsearch
    # MSSQL
    mssqlclient
    impacket-mssqlclient
    # Oracle
    odat
    # PostgreSQL
    pgadmin
    # MySQL
    mysql-client
    # Redis
    redis-cli
    # MongoDB
    mongodb-tools

    # ============ FORENSICS ============
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

    # ============ REVERSE ENGINEERING ============
    ghidra
    radare2
    ida-free
    cutter
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
    # Binary analysis
    checksec
    pwntools
    ropper
    one-gadget
    libc-database
    # Firmware
    binwalk
    firmwalker
    firmware-analysis-toolkit
    # Android
    apktool
    jadx
    dex2jar
    frida
    objection
    # iOS
    ipsw
    ideviceinstaller
    libimobiledevice

    # ============ CONTAINER & CLOUD ============
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

    # ============ REPORTING & DOCUMENTATION ============
    pandoc
    weasyprint
    jupyter
    jupyterlab
    mkdocs
    mkdocs-material
    # Visualization
    graphviz
    mermaid-cli
    plantuml
    # Diagrams
    drawio
    excalidraw

    # ============ UTILITIES ============
    # Terminal
    tmux
    zsh
    fish
    starship
    fzf
    ripgrep
    fd
    bat
    exa
    delta
    # Editors
    neovim
    helix
    micro
    # Git
    git
    git-delta
    lazygit
    gh
    # JSON/YAML
    jq
    yq
    gron
    # HTTP
    curl
    wget
    httpie
    xh
    # Database
    sqlite
    postgresql
    mysql
    redis
    # Programming
    python312
    python312Packages.pip
    python312Packages.virtualenv
    python312Packages.poetry
    nodejs
    nodePackages.npm
    nodePackages.yarn
    nodePackages.pnpm
    golang
    rustc
    cargo
    # Build
    cmake
    meson
    ninja
    make
    gcc
    clang
    lldb
    # Crypto
    gnupg
    age
    sops
    openssl
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
    # Nix
    nix
    nixpkgs-fmt
    statix
  ];
}