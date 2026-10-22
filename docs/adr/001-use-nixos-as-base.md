# 001 - Use NixOS as Base Distribution

## Status
Accepted

## Context
We need a base operating system for Sentinel OS that provides:
- Reproducible, content-addressed builds
- Atomic upgrades and rollbacks
- Declarative system configuration
- Strong package isolation
- Integration with security hardening tooling
- Support for custom kernels and microkernels (seL4)
- Verified boot integration (Heads, TrenchBoot)
- Supply-chain verification (sigstore, SBOMs)

## Decision
We will use **NixOS 24.11 (flakes)** as the base distribution for Sentinel OS.

## Consequences

### Positive
- **Reproducibility**: Identical builds from same inputs; essential for supply-chain security
- **Atomic operations**: Generations enable instant rollback; critical for security updates
- **Declarative config**: System state defined in code; auditable, version-controllable
- **Package isolation**: Per-package store paths prevent dependency conflicts
- **Kernel flexibility**: Easy to swap kernels (hardened, CIP, custom) via overlays
- **Module system**: Composable NixOS modules map cleanly to our architecture
- **Flakes**: Hermetic evaluation, locked dependencies, CI-friendly
- **Binary cache**: Cachix/GitHub Actions integration for fast CI
- **Cross-compilation**: Native support for x86_64, aarch64, riscv64

### Negative
- **Learning curve**: Nix language and ecosystem unfamiliar to many developers
- **Build times**: Initial builds slower than binary distributions (mitigated by caches)
- **Disk usage**: /nix/store grows large; requires garbage collection
- **Imperative escape hatches**: Some tools expect FHS; requires workarounds (steam-run, etc.)
- **seL4 integration**: Not native; requires custom modules and overlays

### Neutral
- **Community size**: Smaller than Debian/Arch but highly technical
- **Documentation**: Excellent wiki but some areas need tribal knowledge

## Alternatives Considered

### Debian/Ubuntu
- **Pros**: Huge package repo, familiar, Kali base
- **Cons**: No atomic upgrades, imperative package management, harder reproducibility, dpkg/apt not content-addressed

### Arch Linux
- **Pros**: Rolling, minimal, AUR
- **Cons**: No atomic rollbacks, no declarative config, pacman not content-addressed

### Fedora Silverblue / Kinoite
- **Pros**: Immutable, rpm-ostree, Flatpak
- **Cons**: rpm-ostree less flexible than Nix, Flatpak sandboxing not sufficient for our compartment model

### Custom Linux From Scratch / Buildroot / Yocto
- **Pros**: Full control, minimal TCB
- **Cons**: Massive maintenance burden, no package ecosystem, reinventing NixOS poorly

### Qubes OS as base
- **Pros**: Xen-based isolation, established security model
- **Cons**: Xen not seL4, dom0 model doesn't match our capability architecture, harder to customize

## Related Decisions
- ADR 002: Use seL4 microkernel for compartment isolation
- ADR 003: Use crosvm as primary VMM backend
- ADR 004: Use llama.cpp/vLLM for local AI inference

## References
- [NixOS Manual](https://nixos.org/manual/nixos/stable/)
- [Nix Flakes](https://nixos.wiki/wiki/Flakes)
- [CIP Project](https://www.cip-project.org/)
- [Reproducible Builds](https://reproducible-builds.org/)

---

## Metadata
- **Date**: 2026-01-15
- **Authors**: Sentinel OS Architecture Team
- **Reviewers**: [TBD]
- **Ticket**: #1