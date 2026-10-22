# Sentinel OS Local Overlays
# This overlay adds Sentinel-specific packages and modifications

{ lib, ... }:

let
  # Import package definitions
  sentinel-pkgs = import ./pkgs { inherit lib; };
in
{
  # Override existing packages with Sentinel-specific versions
  linuxPackages_hardened = sentinel-pkgs.linuxKernel;

  # Add Sentinel-specific packages
  sentinel-capability-broker = sentinel-pkgs.capabilityBroker;
  sentinel-compartment-manager = sentinel-pkgs.compartmentManager;
  sentinel-ai-gateway = sentinel-pkgs.aiGateway;
  sentinel-model-manager = sentinel-pkgs.modelManager;
  sentinel-attestation = sentinel-pkgs.attestation;
  sentinel-health-check = sentinel-pkgs.healthCheck;

  # Hardened tool variants
  nmap = sentinel-pkgs.hardenedNmap;
  masscan = sentinel-pkgs.hardenedMasscan;
  hashcat = sentinel-pkgs.hardenedHashcat;
  john = sentinel-pkgs.hardenedJohn;
  metasploit = sentinel-pkgs.hardenedMetasploit;
  ghidra = sentinel-pkgs.hardenedGhidra;
  radare2 = sentinel-pkgs.hardenedRadare2;
  volatility3 = sentinel-pkgs.hardenedVolatility3;

  # AI/ML packages
  llama-cpp = sentinel-pkgs.llamaCpp;
  vllm = sentinel-pkgs.vllm;
  python312Packages.llama-cpp-python = sentinel-pkgs.llamaCppPython;
  python312Packages.torch = sentinel-pkgs.pytorch;
  python312Packages.onnxruntime = sentinel-pkgs.onnxruntime;

  # seL4 toolchain
  sel4-tools = sentinel-pkgs.sel4Tools;
  crosvm = sentinel-pkgs.crosvm;

  # Verified boot
  heads = sentinel-pkgs.heads;
  trenchboot = sentinel-pkgs.trenchboot;
  sbctl = sentinel-pkgs.sbctl;

  # Security tools
  cosign = sentinel-pkgs.cosign;
  oras = sentinel-pkgs.oras;
  crane = sentinel-pkgs.crane;
  syft = sentinel-pkgs.syft;
  grype = sentinel-pkgs.grype;
  trivy = sentinel-pkgs.trivy;

  # Monitoring
  prometheus-node-exporter = sentinel-pkgs.prometheusNodeExporter;
  vector = sentinel-pkgs.vector;

  # Development
  nil = sentinel-pkgs.nil;
  nixpkgs-fmt = sentinel-pkgs.nixpkgsFmt;
  statix = sentinel-pkgs.statix;
}