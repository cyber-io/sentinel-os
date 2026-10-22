# Sentinel OS

**AI-Native Security Operating System**

[![CI](https://github.com/sentinel-os/sentinel-os/actions/workflows/ci.yml/badge.svg)](https://github.com/sentinel-os/sentinel-os/actions/workflows/ci.yml)
[![Security](https://github.com/sentinel-os/sentinel-os/actions/workflows/security.yml/badge.svg)](https://github.com/sentinel-os/sentinel-os/actions/workflows/security.yml)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPL%203.0-blue.svg)](https://www.gnu.org/licenses/gpl-3.0.html)
[![NixOS](https://img.shields.io/badge/NixOS-24.11-blue.svg)](https://nixos.org/)

---

## Overview

Sentinel OS is a security-first operating system where **AI is not an add-on but the native orchestration layer**. Built on NixOS with seL4 microkernel isolation, it provides:

- **Hardware-rooted trust** via TPM 2.0, measured boot (Heads + TrenchBoot), and kernel lockdown
- **Compartmentalization by default** — every tool, model, and data domain runs in its own VM with capability-based least privilege
- **Local AI inference** — llama.cpp/vLLM with GPU acceleration, signed models via sigstore
- **AI-driven orchestration** — LLM planner with adversarial critics converts intent to verified execution DAGs
- **Threat intelligence engine** — ONNX anomaly detection + LLM explanations + vector case database
- **Immutable & reproducible** — NixOS flakes, atomic upgrades/rollbacks, supply-chain verification

---

## Quick Start

### Prerequisites

- Nix with flakes enabled (`experimental-features = nix-command flakes`)
- x86_64 or aarch64 Linux host
- 16GB+ RAM, 100GB+ disk for development VM

### Development VM

```bash
# Clone
git clone https://github.com/sentinel-os/sentinel-os
cd sentinel-os

# Enter development shell (includes all tools)
nix develop

# Build and run development VM
nix build .#nixosConfigurations.dev-vm.config.system.build.vm
./result/bin/run-*-vm
```

The VM boots with:
- Hardened kernel (KPTI, lockdown, mitigations)
- seL4/crosvm compartment runtime
- Local AI inference stack (llama.cpp + vLLM)
- 50+ security tools pre-installed
- Operator TUI and web-based Mission Control

### Installer ISO

```bash
# Build installer ISO
nix build .#nixosConfigurations.installer.config.system.build.isoImage

# Flash to USB
sudo dd if=result/iso/sentinel-os-*.iso of=/dev/sdX bs=4M status=progress
```

### Hardened Production Build

```bash
# Build hardened configuration
nix build .#nixosConfigurations.hardened.config.system.build.toplevel

# Deploy via nixos-rebuild (on target machine)
sudo nixos-rebuild switch --flake .#hardened
```

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        USER SPACE                                │
│  ┌──────────────┐ ┌──────────────┐ ┌────────────────────────┐  │
│  │ OPERATOR UI  │ │ MISSION CTRL │ │ AI ORCHESTRATION       │  │
│  │ (TUI/Web)    │ │ (Plan/Exec)  │ │ ┌────────────────────┐ │  │
│  └──────┬───────┘ └──────┬───────┘ │ │ LOCAL LLM INFERENCE  │ │  │
│         │                │         │ │ (llama.cpp/vLLM)     │ │  │
│         └───────┬────────┘         │ │ ┌────────────────┐  │ │  │
│                 ▼                │ │ │ TOOL ORCHESTRATOR│  │ │  │
│  ┌──────────────────────────┐    │ │ │ (DAG planner)    │  │ │  │
│  │   CAPABILITY BROKER      │    │ │ └────────────────┘  │ │  │
│  │   (seL4 capabilities)    │    │ │ ┌────────────────┐  │ │  │
│  └────────────┬─────────────┘    │ │ │ THREAT INTEL   │  │ │  │
│               │                  │ │ │ (ML anomaly)    │  │ │  │
│  ┌────────────┴─────────────┐    │ │ └────────────────┘  │ │  │
│  │ COMPARTMENT RUNTIME      │    │ └────────────────────┘  │  │
│  │ ┌────┐ ┌────┐ ┌────┐ ┌──┐ │    │                          │  │
│  │ │Tool│ │Tool│ │LLM │ │DB│ │    │                          │  │
│  │ │ VM │ │ VM │ │ VM │ │VM│ │    │                          │  │
│  │ └────┘ └────┘ └────┘ └──┘ │    │                          │  │
│  └──────────────────────────┘    │                          │  │
├──────────────────────────────────┤                          │  │
│           KERNEL SPACE           │                          │  │
│ ┌─────────┐ ┌─────────┐ ┌──────┐ │ ┌────────────────────┐  │  │
│ │HARDENED │ │VERIFIED │ │CAPABI│ │ │AI ACCELERATION     │  │  │
│ │LINUX    │ │BOOT     │ │LITY  │ │ │(GPU/NPU passthrough)│  │  │
│ │KERNEL   │ │(Heads/  │ │(seL4)│ │ │                    │  │  │
│ └─────────┘ │TrenchBt)│ └──────┘ │ └────────────────────┘  │  │
└──────────────────────────────────┘                          │  │
└──────────────────────────────────────────────────────────────┘
```

---

## Key Components

### Base OS (`sentinel/modules/base.nix`)
- NixOS 24.11 with hardened systemd, AppArmor, auditd
- Btrfs with atomic snapshots
- Network locked down by default
- TPM 2.0, entropy, SSH hardening

### Kernel (`sentinel/modules/kernel/hardened.nix`)
- CIP SLTS kernel (6.6.x) with full hardening
- KPTI, Spectre/Meltdown mitigations, KASAN/KCSAN (paranoid mode)
- Module signing enforcement, lockdown=confidentiality
- IOMMU, VFIO for device passthrough

### Verified Boot (`sentinel/modules/verified-boot.nix`)
- Heads firmware for measured boot
- TrenchBoot D-RTM (Intel TXT/AMD SKINIT)
- sbctl for Secure Boot key management
- IMA/EVM runtime integrity measurement
- PCR policy for sealed secrets

### Compartments (`sentinel/modules/compartments.nix`)
- crosvm/QEMU/Kata Containers backend
- Capability broker for least-privilege access
- Network isolation via VLANs
- GPU/NPU mediated passthrough
- AppArmor profiles per compartment type

### AI Inference (`sentinel/modules/ai/inference.nix`)
- llama.cpp server (CPU/GPU) on port 8080
- vLLM server (multi-GPU) on port 8000
- Ollama-compatible gateway on port 11434
- Model registry (ORAS + cosign)
- Priority-based scheduler with preemption
- Prometheus metrics

### Tools (`sentinel/modules/tools/`)
- **minimal**: nmap, masscan, curl, basic utils
- **core**: + nuclei, ffuf, hashcat, metasploit, bloodhound, ghidra
- **full**: 200+ tools across all categories
- **forensics/wireless/cloud**: Specialized profiles

---

## Configurations

| Config | Purpose | Kernel | AI | Compartments | Tools |
|--------|---------|--------|-----|--------------|-------|
| `dev-vm` | Development | Standard | ✓ | ✓ | core |
| `installer` | ISO installer | Hardened | ✗ | ✗ | minimal |
| `hardened` | Production | Hardened+ | ✓ | ✓ | full |

---

## Directory Structure

```
sentinel-os/
├── flake.nix                    # Main flake entry point
├── .github/workflows/ci.yml     # CI/CD pipeline
├── .pre-commit-config.yaml      # Pre-commit hooks
├── .markdownlint.json           # Markdown linting rules
├── SENTINEL-OS-SPEC.md          # Architecture specification
├── docs/
│   └── adr/                     # Architecture Decision Records
├── sentinel/
│   ├── modules/                 # NixOS modules
│   │   ├── base.nix
│   │   ├── kernel/hardened.nix
│   │   ├── verified-boot.nix
│   │   ├── compartments.nix
│   │   ├── ai/inference.nix
│   │   └── tools/{core.nix,all.nix}
│   ├── configurations/          # NixOS configurations
│   │   ├── dev-vm.nix
│   │   ├── installer.nix
│   │   └── hardened.nix
│   ├── overlays/                # Package overlays
│   │   ├── default.nix
│   │   └── pkgs/default.nix
│   └── pkgs/                    # Custom package definitions (future)
└── compartments/                # seL4 component configs (future)
```

---

## Development

### Environment

```bash
# Enter dev shell with all tools
nix develop

# Or use direnv (auto-loads on cd)
direnv allow
```

### Formatting & Linting

```bash
# Format Nix files
nixpkgs-fmt .

# Static analysis
statix check .
deadnix .

# Shell scripts
shellcheck scripts/*.sh
shfmt -w scripts/*.sh

# Markdown
markdownlint **/*.md
```

### Testing

```bash
# Build all configurations
nix build .#nixosConfigurations.dev-vm.config.system.build.toplevel
nix build .#nixosConfigurations.installer.config.system.build.isoImage
nix build .#nixosConfigurations.hardened.config.system.build.toplevel

# Run VM tests
./result/bin/run-*-vm
```

### Adding Tools

1. Add package to `sentinel/modules/tools/core.nix` or `all.nix`
2. Create capability manifest in `sentinel/modules/compartments.nix`
3. Add AppArmor profile if needed
4. Test in dev-vm

### Adding AI Models

```bash
# Pull model from registry
sentinel-model pull llama-3.1-8b-instruct

# Or configure in flake
sentinel.ai.models.default = [
  { name = "my-model"; source = "org/model-GGUF"; filename = "model.Q4_K_M.gguf"; }
];
```

---

## Security

### Threat Model

- **Physical access**: Verified boot, TPM sealing, encrypted root
- **Supply chain**: Reproducible builds, sigstore signing, SBOMs
- **Runtime compromise**: seL4 isolation, capability-based access, AppArmor
- **AI/ML risks**: Local inference only, model verification, sandboxed execution
- **Data exfiltration**: Network segmentation, egress control, audit logging

### Hardening Checklist

- [ ] Kernel lockdown=confidentiality
- [ ] Module signature enforcement
- [ ] Secure Boot enrolled
- [ ] TPM 2.0 owned and measured
- [ ] IMA/EVM appraising
- [ ] AppArmor enforcing
- [ ] Auditd logging all syscalls
- [ ] Network default-deny
- [ ] Compartment capability manifests
- [ ] AI models signed and verified
- [ ] GPU/NPU isolated via VFIO

### Reporting Vulnerabilities

Email: security@sentinel-os.org (PGP key in KEYS file)

---

## Roadmap

| Phase | Timeline | Focus |
|-------|----------|-------|
| **0: Foundation** | Months 0-3 | NixOS flake, hardened kernel, seL4/crosvm, verified boot, CI |
| **1: AI Core** | Months 3-9 | Local LLM stack, model registry, planner+critics, operator TUI, 10 tools |
| **2: Operator UX** | Months 9-18 | Mission Control web UI, Copilot, threat intel engine, 50+ tools |
| **3: Hardening** | Months 18-36 | Formal verification, confidential compute (TDX/SEV-SNP), ARM64, 200+ tools |
| **4: Production** | Year 3+ | Graphical installer, OTA updates, community infra, certifications |

---

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make changes with tests
4. Run pre-commit: `pre-commit run --all-files`
5. Submit PR with description

### Commit Convention

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(compartments): add GPU scheduling for AI workloads
fix(kernel): resolve KPTI regression on AMD Zen 4
docs(adr): record decision on seL4 vs KVM isolation
```

### Architecture Decision Records

Significant decisions are recorded in `docs/adr/` using the MADR format.

---

## License

GPL-3.0-or-later for OS components; Apache-2.0 for AI components; MIT for SDKs.

See [LICENSE](LICENSE) for details.

---

## Acknowledgments

- **NixOS** — Reproducible builds, declarative configuration
- **seL4** — Formally verified microkernel
- **CIP** — Civil Infrastructure Platform kernel
- **Heads/TrenchBoot** — Verified boot
- **llama.cpp/vLLM** — Local inference
- **sigstore/cosign** — Supply chain security
- **All security tool authors** — The giants whose shoulders we stand on

---

## Community

- **Matrix**: #sentinel-os:matrix.org
- **Discourse**: discuss.sentinel-os.org
- **GitHub**: github.com/sentinel-os/sentinel-os
- **Mailing list**: sentinel-dev@lists.sentinel-os.org

---

*Built with 🦀 and 🔐 by the Sentinel OS team*