# 002 - Use seL4 Microkernel for Compartment Isolation

## Status
Accepted

## Context
We need strong isolation between compartments (tools, AI models, data domains, network zones). Options:
- Linux namespaces/cgroups (Docker, containerd)
- KVM/QEMU full virtualization
- crosvm / cloud-hypervisor (lightweight VMMs)
- Kata Containers (VM-based containers)
- gVisor (user-space kernel)
- seL4 microkernel (formally verified)
- Custom capability system on Linux

Requirements:
- Mathematical proof of isolation (no shared kernel state)
- Capability-based access control (least privilege)
- Minimal TCB (Trusted Computing Base)
- GPU/NPU device passthrough
- Good Rust/tooling ecosystem
- Active maintenance and community

## Decision
We will use **seL4 microkernel** as the isolation boundary for compartments, with **crosvm** as the primary VMM backend for running Linux guest VMs as compartments.

## Consequences

### Positive
- **Formal verification**: seL4 is the only general-purpose OS kernel with machine-checked proof of functional correctness and isolation
- **Capability model**: Native capability-based security matches our architecture exactly
- **Minimal TCB**: ~10k lines of verified kernel vs millions in Linux
- **No shared kernel state**: True isolation; compromise of one compartment cannot directly affect others
- **Mixed criticality**: Can run high-assurance and legacy components side-by-side
- **crosvm integration**: Rust-based VMM, minimal, supports virtio, GPU passthrough, vsock
- **seL4 CPL**: Component Platform Layer for easier component composition

### Negative
- **Driver support**: seL4 has limited hardware drivers; rely on Linux guests for device support
- **Development complexity**: Requires seL4 expertise; smaller talent pool
- **Tooling maturity**: Less mature than Linux KVM/QEMU ecosystem
- **Boot complexity**: Requires custom boot chain (Heads → TrenchBoot → seL4)
- **Debugging**: Harder to debug cross-VM issues
- **Performance overhead**: Microkernel IPC vs monolithic syscalls (mitigated by batching)

### Neutral
- **Linux guests still needed**: Compartments run Linux VMs; seL4 isolates the VMMs
- **Gradual adoption**: Can start with crosvm/KVM on Linux, migrate to seL4 incrementally
- **Certification path**: seL4 has Common Criteria EAL7; aligns with our certification goals

## Alternatives Considered

### KVM/QEMU on Hardened Linux
- **Pros**: Mature, excellent performance, huge ecosystem, GPU passthrough works
- **Cons**: Shared kernel = shared attack surface; no formal isolation proof; namespace escapes possible

### Kata Containers
- **Pros**: OCI-compatible, Kubernetes integration, VM isolation
- **Cons**: Still uses Linux kernel in guest; no formal verification; heavier than crosvm

### gVisor
- **Pros**: User-space kernel, syscall interception, good compatibility
- **Cons**: Performance overhead; not formally verified; limited syscall coverage; no GPU passthrough

### Custom Capability System on Linux (Capsicum, Landlock)
- **Pros**: Runs on standard Linux, incremental adoption
- **Cons**: Linux kernel still shared; Landlock limited to filesystem; Capsicum FreeBSD-only

### Firecracker / Cloud Hypervisor
- **Pros**: Lightweight, fast boot, Rust, AWS-backed
- **Cons**: No formal verification; designed for serverless, not interactive security tools

## Related Decisions
- ADR 001: Use NixOS as base distribution
- ADR 003: Use crosvm as primary VMM backend
- ADR 005: Use capability broker for compartment authorization

## References
- [seL4](https://sel4.systems/)
- [seL4 Verification](https://sel4.systems/Verification/)
- [crosvm](https://github.com/google/crosvm)
- [seL4 CPL](https://github.com/seL4/cpl)
- [Capability Myths](https://capabilitymyths.org/)

---

## Metadata
- **Date**: 2026-01-15
- **Authors**: Sentinel OS Architecture Team
- **Reviewers**: [TBD]
- **Ticket**: #2