{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.sentinel;
  securityLevel = cfg.securityLevel;
in
{
  # Kernel package selection based on security level
  boot.kernelPackages = pkgs.linuxPackages_hardened;

  # Kernel parameters for hardening
  boot.kernelParams = [
    # Memory protection
    "slab_nomerge"
    "init_on_alloc=1"
    "init_on_free=1"
    "page_poison=1"
    "vsyscall=none"

    # Module signing enforcement
    "module.sig_enforce=1"
    "module.sig_hash=sha512"

    # Lockdown mode
    "lockdown=confidentiality"

    # KPTI (Kernel Page Table Isolation)
    "kpti=on"

    # Spectre/Meltdown mitigations
    "spectre_v2=on"
    "spec_store_bypass_disable=on"
    "tsx=off"
    "tsx_async_abort=full,nosmt"
    "mds=full,nosmt"
    "l1tf=full,force"
    "mmio_stale_data=on"
    "srbds=on"

    # Stack protection
    "randomize_kstack_offset=on"

    # Disable dangerous features
    "module_blacklist=firewire-core,firewire-ohci,firewire-sbp2,thunderbolt"
    "sysrq_always_enabled=0"

    # Hardened usercopy
    "hardened_usercopy=on"

    # Page table isolation for kernel
    "pti=on"

    # Disable kexec (unless signed)
    "kexec_load_disabled=1"

    # Restrict kernel logs
    "dmesg_restrict=1"

    # Kernel address space layout randomization
    "kaslr"
    "nokaslr=0"
  ];

  # Sysctl hardening
  boot.kernel.sysctl = {
    # Kernel
    "kernel.dmesg_restrict" = 1;
    "kernel.kptr_restrict" = 2;
    "kernel.perf_event_paranoid" = 3;
    "kernel.kexec_load_disabled" = 1;
    "kernel.sysrq" = 0;
    "kernel.unprivileged_bpf_disabled" = 1;
    "kernel.unprivileged_userns_clone" = 0;
    "kernel.yama.ptrace_scope" = 3;
    "kernel.modules_disabled" = 1;
    "kernel.deny_new_usb" = 1;

    # VM
    "vm.mmap_rnd_bits" = 32;
    "vm.mmap_rnd_compat_bits" = 16;
    "vm.unprivileged_userfaultfd" = 0;

    # Network
    "net.core.bpf_jit_harden" = 2;
    "net.ipv4.conf.all.rp_filter" = 1;
    "net.ipv4.conf.default.rp_filter" = 1;
    "net.ipv4.conf.all.accept_source_route" = 0;
    "net.ipv4.conf.default.accept_source_route" = 0;
    "net.ipv4.conf.all.accept_redirects" = 0;
    "net.ipv4.conf.default.accept_redirects" = 0;
    "net.ipv4.conf.all.secure_redirects" = 0;
    "net.ipv4.conf.default.secure_redirects" = 0;
    "net.ipv4.conf.all.send_redirects" = 0;
    "net.ipv4.conf.default.send_redirects" = 0;
    "net.ipv4.icmp_echo_ignore_broadcasts" = 1;
    "net.ipv4.icmp_ignore_bogus_error_responses" = 1;
    "net.ipv4.tcp_syncookies" = 1;
    "net.ipv6.conf.all.accept_ra" = 0;
    "net.ipv6.conf.default.accept_ra" = 0;

    # FS
    "fs.protected_hardlinks" = 1;
    "fs.protected_symlinks" = 1;
    "fs.suid_dumpable" = 0;

    # Device
    "dev.tty.ldisc_autoload" = 0;
  };

  # Kernel configuration fragments (merged with base kernel config)
  boot.kernel.config = let
    # Base hardening config
    baseHardening = {
      # Memory protection
      CONFIG_DEBUG_RODATA = lib.kernelEnable;
      CONFIG_DEBUG_SET_MODULE_RONX = lib.kernelEnable;
      CONFIG_DEBUG_WX = lib.kernelEnable;
      CONFIG_STRICT_DEVMEM = lib.kernelEnable;
      CONFIG_STRICT_KERNEL_RWX = lib.kernelEnable;
      CONFIG_STRICT_MODULE_RWX = lib.kernelEnable;
      CONFIG_HARDENED_USERCOPY = lib.kernelEnable;
      CONFIG_FORTIFY_SOURCE = lib.kernelEnable;
      CONFIG_STATIC_USERMODEHELPER = lib.kernelEnable;
      CONFIG_STATIC_USERMODEHELPER_PATH = '""';

      # Stack protection
      CONFIG_CC_STACKPROTECTOR_STRONG = lib.kernelEnable;
      CONFIG_STACKPROTECTOR = lib.kernelEnable;
      CONFIG_STACKPROTECTOR_STRONG = lib.kernelEnable;

      # Heap protection
      CONFIG_SLAB_FREELIST_RANDOM = lib.kernelEnable;
      CONFIG_SLAB_FREELIST_HARDENED = lib.kernelEnable;
      CONFIG_SHUFFLE_PAGE_ALLOCATOR = lib.kernelEnable;
      CONFIG_PAGE_POISONING = lib.kernelEnable;
      CONFIG_PAGE_POISONING_NO_SANITY = lib.kernelDisable;
      CONFIG_PAGE_POISONING_ZERO = lib.kernelEnable;

      # KASAN (Kernel Address Sanitizer) - paranoid only
      CONFIG_KASAN = lib.mkIf (securityLevel == "paranoid") lib.kernelEnable;
      CONFIG_KASAN_INLINE = lib.mkIf (securityLevel == "paranoid") lib.kernelEnable;
      CONFIG_KASAN_STACK = lib.mkIf (securityLevel == "paranoid") 1;
      CONFIG_KASAN_VMALLOC = lib.mkIf (securityLevel == "paranoid") lib.kernelEnable;

      # KCSAN (Concurrency Sanitizer) - paranoid only
      CONFIG_KCSAN = lib.mkIf (securityLevel == "paranoid") lib.kernelEnable;

      # Hardening
      CONFIG_HARDENED_USERCOPY = lib.kernelEnable;
      CONFIG_HARDENED_USERCOPY_FALLBACK = lib.kernelDisable;
      CONFIG_HARDENED_USERCOPY_PAGESPAN = lib.kernelEnable;
      CONFIG_REFCOUNT_FULL = lib.kernelEnable;

      # Lockdown
      CONFIG_SECURITY_LOCKDOWN_LSM = lib.kernelEnable;
      CONFIG_SECURITY_LOCKDOWN_LSM_EARLY = lib.kernelEnable;
      CONFIG_LOCK_DOWN_KERNEL_FORCE_CONFIDENTIALITY = lib.kernelEnable;

      # Module signing
      CONFIG_MODULE_SIG = lib.kernelEnable;
      CONFIG_MODULE_SIG_ALL = lib.kernelEnable;
      CONFIG_MODULE_SIG_SHA512 = lib.kernelEnable;
      CONFIG_MODULE_SIG_KEY = '""'; # Will be set at build time

      # BPF hardening
      CONFIG_BPF_JIT_HARDEN = lib.kernelEnable;
      CONFIG_BPF_JIT_ALWAYS_ON = lib.kernelEnable;

      # KPTI
      CONFIG_PAGE_TABLE_ISOLATION = lib.kernelEnable;

      # Spectre/Meltdown
      CONFIG_RETPOLINE = lib.kernelEnable;
      CONFIG_SPECULATION_MITIGATION = lib.kernelEnable;
      CONFIG_X86_INTEL_UMIP = lib.kernelEnable;
      CONFIG_X86_SMAP = lib.kernelEnable;
      CONFIG_X86_SMEP = lib.kernelEnable;
      CONFIG_X86_INTEL_MPX = lib.kernelDisable; # Deprecated, security risk

      # User namespace restrictions
      CONFIG_USER_NS = lib.kernelEnable;
      CONFIG_MAX_USER_NAMESPACES = 0;

      # Filesystem protections
      CONFIG_PROC_FS = lib.kernelEnable;
      CONFIG_PROC_SYSCTL = lib.kernelEnable;
      CONFIG_PROC_PAGE_MONITOR = lib.kernelEnable;
      CONFIG_PROC_CHILDREN = lib.kernelEnable;
      CONFIG_KERNFS = lib.kernelEnable;
      CONFIG_SYSFS = lib.kernelEnable;
      CONFIG_TMPFS = lib.kernelEnable;
      CONFIG_TMPFS_POSIX_ACL = lib.kernelEnable;
      CONFIG_TMPFS_XATTR = lib.kernelEnable;
      CONFIG_HUGETLBFS = lib.kernelEnable;
      CONFIG_HUGETLB_PAGE = lib.kernelEnable;

      # Disable dangerous filesystems
      CONFIG_AFFS_FS = lib.kernelDisable;
      CONFIG_ECRYPT_FS = lib.kernelDisable;
      CONFIG_HFS_FS = lib.kernelDisable;
      CONFIG_HFSPLUS_FS = lib.kernelDisable;
      CONFIG_JFFS2_FS = lib.kernelDisable;
      CONFIG_JFS_FS = lib.kernelDisable;
      CONFIG_MINIX_FS = lib.kernelDisable;
      CONFIG_NILFS2_FS = lib.kernelDisable;
      CONFIG_OMFS_FS = lib.kernelDisable;
      CONFIG_QNX4FS_FS = lib.kernelDisable;
      CONFIG_QNX6FS_FS = lib.kernelDisable;
      CONFIG_ROMFS_FS = lib.kernelDisable;
      CONFIG_SYSV_FS = lib.kernelDisable;
      CONFIG_UFS_FS = lib.kernelDisable;
      CONFIG_UBIFS_FS = lib.kernelDisable;
      CONFIG_UFS_FS = lib.kernelDisable;
      CONFIG_EXOFS_FS = lib.kernelDisable;
      CONFIG_F2FS_FS = lib.kernelDisable;
      CONFIG_NFSD = lib.kernelDisable;
      CONFIG_NFS_V2 = lib.kernelDisable;
      CONFIG_NFS_V3 = lib.kernelDisable;
      CONFIG_NFS_V4 = lib.kernelDisable;
      CONFIG_CIFS = lib.kernelDisable;
      CONFIG_CODA_FS = lib.kernelDisable;
      CONFIG_AFS_FS = lib.kernelDisable;
      CONFIG_9P_FS = lib.kernelDisable;

      # Network hardening
      CONFIG_INET = lib.kernelEnable;
      CONFIG_INET_DIAG = lib.kernelEnable;
      CONFIG_INET_TCP_DIAG = lib.kernelEnable;
      CONFIG_INET_UDP_DIAG = lib.kernelEnable;
      CONFIG_INET_RAW_DIAG = lib.kernelEnable;
      CONFIG_TCP_CONG_ADVANCED = lib.kernelEnable;
      CONFIG_TCP_CONG_BBR = lib.kernelEnable;
      CONFIG_DEFAULT_BBR = lib.kernelEnable;
      CONFIG_IPV6 = lib.kernelEnable;
      CONFIG_IPV6_ROUTER_PREF = lib.kernelEnable;
      CONFIG_IPV6_ROUTE_INFO = lib.kernelEnable;
      CONFIG_IPV6_OPTIMISTIC_DAD = lib.kernelEnable;
      CONFIG_INET6_AH = lib.kernelEnable;
      CONFIG_INET6_ESP = lib.kernelEnable;
      CONFIG_INET6_IPCOMP = lib.kernelEnable;
      CONFIG_IPV6_MIP6 = lib.kernelDisable;
      CONFIG_IPV6_ILA = lib.kernelDisable;
      CONFIG_INET6_XFRM_TUNNEL = lib.kernelEnable;
      CONFIG_INET6_TUNNEL = lib.kernelEnable;
      CONFIG_INET6_XFRM_MODE_TRANSPORT = lib.kernelEnable;
      CONFIG_INET6_XFRM_MODE_TUNNEL = lib.kernelEnable;
      CONFIG_INET6_XFRM_MODE_BEET = lib.kernelEnable;
      CONFIG_INET6_XFRM_MODE_ROUTEOPTIMIZATION = lib.kernelEnable;
      CONFIG_IPV6_VTI = lib.kernelEnable;
      CONFIG_IPV6_SIT = lib.kernelEnable;
      CONFIG_IPV6_SIT_6RD = lib.kernelEnable;
      CONFIG_IPV6_NDISC_NODETYPE = lib.kernelEnable;
      CONFIG_IPV6_TUNNEL = lib.kernelEnable;
      CONFIG_IPV6_GRE = lib.kernelEnable;
      CONFIG_IPV6_FOU = lib.kernelEnable;
      CONFIG_IPV6_FOU_TUNNEL = lib.kernelEnable;
      CONFIG_IPV6_MULTIPLE_TABLES = lib.kernelEnable;
      CONFIG_IPV6_SUBTREES = lib.kernelEnable;
      CONFIG_IPV6_MROUTE = lib.kernelDisable;
      CONFIG_IPV6_MROUTE_MULTIPLE_TABLES = lib.kernelDisable;
      CONFIG_IPV6_PIMSM_V2 = lib.kernelDisable;
      CONFIG_IPV6_SEG6_LWTUNNEL = lib.kernelEnable;
      CONFIG_IPV6_SEG6_HMAC = lib.kernelEnable;
      CONFIG_IPV6_SEG6_BPF = lib.kernelEnable;
      CONFIG_NETLABEL = lib.kernelEnable;
      CONFIG_NETWORK_SECMARK = lib.kernelEnable;
      CONFIG_NET_PTP_CLASSIFY = lib.kernelEnable;
      CONFIG_NETWORK_PHY_TIMESTAMPING = lib.kernelEnable;

      # Disable legacy protocols
      CONFIG_ATALK = lib.kernelDisable;
      CONFIG_AX25 = lib.kernelDisable;
      CONFIG_BATMAN_ADV = lib.kernelDisable;
      CONFIG_BLUETOOTH = lib.mkIf (securityLevel != "paranoid") lib.kernelEnable;
      CONFIG_CAN = lib.kernelDisable;
      CONFIG_DECNET = lib.kernelDisable;
      CONFIG_DNET = lib.kernelDisable;
      CONFIG_ECONET = lib.kernelDisable;
      CONFIG_IEEE802154 = lib.kernelDisable;
      CONFIG_IRDA = lib.kernelDisable;
      CONFIG_L2TP = lib.kernelDisable;
      CONFIG_LLC = lib.kernelDisable;
      CONFIG_MAC80211 = lib.kernelDisable;
      CONFIG_MAC802154 = lib.kernelDisable;
      CONFIG_NET_9P = lib.kernelDisable;
      CONFIG_NET_DSA = lib.kernelDisable;
      CONFIG_NET_SCH_INGRESS = lib.kernelEnable;
      CONFIG_NET_SCHED = lib.kernelEnable;
      CONFIG_NETFILTER = lib.kernelEnable;
      CONFIG_NETFILTER_ADVANCED = lib.kernelEnable;
      CONFIG_NETFILTER_NETLINK = lib.kernelEnable;
      CONFIG_NETFILTER_NETLINK_ACCT = lib.kernelEnable;
      CONFIG_NETFILTER_NETLINK_QUEUE = lib.kernelEnable;
      CONFIG_NETFILTER_NETLINK_LOG = lib.kernelEnable;
      CONFIG_NETFILTER_XT_TARGET_LOG = lib.kernelEnable;
      CONFIG_NETFILTER_XT_TARGET_NFLOG = lib.kernelEnable;
      CONFIG_NETFILTER_XT_MATCH_CONNTRACK = lib.kernelEnable;
      CONFIG_NETFILTER_XT_MATCH_STATE = lib.kernelEnable;
      CONFIG_NF_CONNTRACK = lib.kernelEnable;
      CONFIG_NF_CONNTRACK_MARK = lib.kernelEnable;
      CONFIG_NF_CONNTRACK_SECMARK = lib.kernelEnable;
      CONFIG_NF_CONNTRACK_ZONES = lib.kernelEnable;
      CONFIG_NF_CONNTRACK_PROCFS = lib.kernelEnable;
      CONFIG_NF_CONNTRACK_EVENTS = lib.kernelEnable;
      CONFIG_NF_CONNTRACK_TIMEOUT = lib.kernelEnable;
      CONFIG_NF_CONNTRACK_TIMESTAMP = lib.kernelEnable;
      CONFIG_NF_CONNTRACK_LABELS = lib.kernelEnable;
      CONFIG_NF_CT_PROTO_DCCP = lib.kernelEnable;
      CONFIG_NF_CT_PROTO_GRE = lib.kernelEnable;
      CONFIG_NF_CT_PROTO_SCTP = lib.kernelEnable;
      CONFIG_NF_CT_PROTO_UDPLITE = lib.kernelEnable;
      CONFIG_NF_NAT = lib.kernelEnable;
      CONFIG_NF_NAT_AMANDA = lib.kernelDisable;
      CONFIG_NF_NAT_FTP = lib.kernelDisable;
      CONFIG_NF_NAT_IRC = lib.kernelDisable;
      CONFIG_NF_NAT_SIP = lib.kernelDisable;
      CONFIG_NF_NAT_TFTP = lib.kernelDisable;
      CONFIG_NF_NAT_REDIRECT = lib.kernelEnable;
      CONFIG_NF_TABLES = lib.kernelEnable;
      CONFIG_NF_TABLES_INET = lib.kernelEnable;
      CONFIG_NF_TABLES_NETDEV = lib.kernelEnable;
      CONFIG_NFT_NUMGEN = lib.kernelEnable;
      CONFIG_NFT_CT = lib.kernelEnable;
      CONFIG_NFT_FLOW_OFFLOAD = lib.kernelEnable;
      CONFIG_NFT_COUNTER = lib.kernelEnable;
      CONFIG_NFT_LOG = lib.kernelEnable;
      CONFIG_NFT_LIMIT = lib.kernelEnable;
      CONFIG_NFT_MASQ = lib.kernelEnable;
      CONFIG_NFT_REDIR = lib.kernelEnable;
      CONFIG_NFT_NAT = lib.kernelEnable;
      CONFIG_NFT_OBJREF = lib.kernelEnable;
      CONFIG_NFT_QUEUE = lib.kernelEnable;
      CONFIG_NFT_QUOTA = lib.kernelEnable;
      CONFIG_NFT_REJECT = lib.kernelEnable;
      CONFIG_NFT_REJECT_INET = lib.kernelEnable;
      CONFIG_NFT_COMPAT = lib.kernelEnable;
      CONFIG_NFT_HASH = lib.kernelEnable;
      CONFIG_NFT_FIB = lib.kernelEnable;
      CONFIG_NFT_FIB_INET = lib.kernelEnable;
      CONFIG_NFT_XFRM = lib.kernelEnable;
      CONFIG_NFT_SOCKET = lib.kernelEnable;
      CONFIG_NFT_OSF = lib.kernelEnable;
      CONFIG_NFT_TPROXY = lib.kernelEnable;
      CONFIG_NF_DUP_NETDEV = lib.kernelEnable;
      CONFIG_NF_DUP_IPV4 = lib.kernelEnable;
      CONFIG_NF_DUP_IPV6 = lib.kernelEnable;
      CONFIG_NF_REJECT_IPV4 = lib.kernelEnable;
      CONFIG_NF_REJECT_IPV6 = lib.kernelEnable;
      CONFIG_NF_LOG_SYSLOG = lib.kernelEnable;
      CONFIG_NF_LOG_NETDEV = lib.kernelEnable;
      CONFIG_NF_LOG_COMMON = lib.kernelEnable;
      CONFIG_NETFILTER_XTABLES = lib.kernelEnable;

      # Crypto
      CONFIG_CRYPTO = lib.kernelEnable;
      CONFIG_CRYPTO_ALGAPI = lib.kernelEnable;
      CONFIG_CRYPTO_AEAD = lib.kernelEnable;
      CONFIG_CRYPTO_BLKCIPHER = lib.kernelEnable;
      CONFIG_CRYPTO_HASH = lib.kernelEnable;
      CONFIG_CRYPTO_RNG = lib.kernelEnable;
      CONFIG_CRYPTO_RNG_DEFAULT = lib.kernelEnable;
      CONFIG_CRYPTO_AKCIPHER = lib.kernelEnable;
      CONFIG_CRYPTO_KPP = lib.kernelEnable;
      CONFIG_CRYPTO_ACOMP = lib.kernelEnable;
      CONFIG_CRYPTO_MANAGER = lib.kernelEnable;
      CONFIG_CRYPTO_MANAGER2 = lib.kernelEnable;
      CONFIG_CRYPTO_USER = lib.kernelEnable;
      CONFIG_CRYPTO_MANAGER_DISABLE_TESTS = lib.kernelEnable;
      CONFIG_CRYPTO_GF128MUL = lib.kernelEnable.
      CONFIG_CRYPTO_NULL = lib.kernelDisable.
      CONFIG_CRYPTO_NULL2 = lib.kernelDisable.
      CONFIG_CRYPTO_CRYPTD = lib.kernelEnable.
      CONFIG_CRYPTO_AUTHENC = lib.kernelEnable.
      CONFIG_CRYPTO_TEST = lib.kernelDisable.
      CONFIG_CRYPTO_SIMD = lib.kernelEnable.
      CONFIG_CRYPTO_GLUE_HELPER_X86 = lib.kernelEnable.
      CONFIG_CRYPTO_ENGINE = lib.kernelEnable.
      # Algorithms
      CONFIG_CRYPTO_AES = lib.kernelEnable.
      CONFIG_CRYPTO_AES_TI = lib.kernelEnable.
      CONFIG_CRYPTO_AES_NI_INTEL = lib.kernelEnable.
      CONFIG_CRYPTO_ANUBIS = lib.kernelDisable.
      CONFIG_CRYPTO_ARC4 = lib.kernelDisable.
      CONFIG_CRYPTO_BLOWFISH = lib.kernelDisable.
      CONFIG_CRYPTO_CAMELLIA = lib.kernelDisable.
      CONFIG_CRYPTO_CAST5 = lib.kernelDisable.
      CONFIG_CRYPTO_CAST6 = lib.kernelDisable.
      CONFIG_CRYPTO_DES = lib.kernelDisable.
      CONFIG_CRYPTO_FCRYPT = lib.kernelDisable.
      CONFIG_CRYPTO_KHAZAD = lib.kernelDisable.
      CONFIG_CRYPTO_SALSA20 = lib.kernelEnable.
      CONFIG_CRYPTO_CHACHA20 = lib.kernelEnable.
      CONFIG_CRYPTO_SEED = lib.kernelDisable.
      CONFIG_CRYPTO_SERPENT = lib.kernelDisable.
      CONFIG_CRYPTO_SM4 = lib.kernelDisable.
      CONFIG_CRYPTO_TEA = lib.kernelDisable.
      CONFIG_CRYPTO_TWOFISH = lib.kernelDisable.
      CONFIG_CRYPTO_SM3 = lib.kernelDisable.
      CONFIG_CRYPTO_STREEBOG = lib.kernelDisable.
      CONFIG_CRYPTO_TGR192 = lib.kernelDisable.
      CONFIG_CRYPTO_WP512 = lib.kernelDisable.
      CONFIG_CRYPTO_GHASH = lib.kernelEnable.
      CONFIG_CRYPTO_POLY1305 = lib.kernelEnable.
      CONFIG_CRYPTO_MD4 = lib.kernelDisable.
      CONFIG_CRYPTO_MD5 = lib.kernelDisable.
      CONFIG_CRYPTO_MICHAEL_MIC = lib.kernelDisable.
      CONFIG_CRYPTO_RMD128 = lib.kernelDisable.
      CONFIG_CRYPTO_RMD160 = lib.kernelDisable.
      CONFIG_CRYPTO_RMD256 = lib.kernelDisable.
      CONFIG_CRYPTO_RMD320 = lib.kernelDisable.
      CONFIG_CRYPTO_SHA1 = lib.kernelDisable.
      CONFIG_CRYPTO_SHA256 = lib.kernelEnable.
      CONFIG_CRYPTO_SHA512 = lib.kernelEnable.
      CONFIG_CRYPTO_SHA3 = lib.kernelEnable.
      CONFIG_CRYPTO_SM3 = lib.kernelDisable.
      CONFIG_CRYPTO_STREEBOG = lib.kernelDisable.
      CONFIG_CRYPTO_TGR192 = lib.kernelDisable.
      CONFIG_CRYPTO_WP512 = lib.kernelDisable.
      CONFIG_CRYPTO_GHASH = lib.kernelEnable.
      CONFIG_CRYPTO_POLY1305 = lib.kernelEnable.
      CONFIG_CRYPTO_CRC32C = lib.kernelEnable.
      CONFIG_CRYPTO_CRC32C_INTEL = lib.kernelEnable.
      CONFIG_CRYPTO_CRC32 = lib.kernelEnable.
      CONFIG_CRYPTO_CRC32_PCLMUL = lib.kernelEnable.
      CONFIG_CRYPTO_CRCT10DIF = lib.kernelEnable.
      CONFIG_CRYPTO_CRCT10DIF_PCLMUL = lib.kernelEnable.
      CONFIG_CRYPTO_GHASH = lib.kernelEnable.
      CONFIG_CRYPTO_POLY1305 = lib.kernelEnable.
      CONFIG_CRYPTO_AES_GCM = lib.kernelEnable.
      CONFIG_CRYPTO_CHACHA20_POLY1305 = lib.kernelEnable.
      CONFIG_CRYPTO_ECHAINIV = lib.kernelEnable.
      CONFIG_CRYPTO_GCM = lib.kernelEnable.
      CONFIG_CRYPTO_SEQIV = lib.kernelEnable.
      CONFIG_CRYPTO_ESSIV = lib.kernelEnable.
      CONFIG_CRYPTO_CBC = lib.kernelEnable.
      CONFIG_CRYPTO_CTR = lib.kernelEnable.
      CONFIG_CRYPTO_CTS = lib.kernelEnable.
      CONFIG_CRYPTO_ECB = lib.kernelDisable.
      CONFIG_CRYPTO_LRW = lib.kernelDisable.
      CONFIG_CRYPTO_OFB = lib.kernelDisable.
      CONFIG_CRYPTO_PCBC = lib.kernelDisable.
      CONFIG_CRYPTO_XTS = lib.kernelEnable.
      CONFIG_CRYPTO_KEYWRAP = lib.kernelEnable.
      CONFIG_CRYPTO_NHPOLY1305 = lib.kernelEnable.
      CONFIG_CRYPTO_ADIANTUM = lib.kernelEnable.
      CONFIG_CRYPTO_ESSIV = lib.kernelEnable.
      CONFIG_CRYPTO_HMAC = lib.kernelEnable.
      CONFIG_CRYPTO_XCBC = lib.kernelEnable.
      CONFIG_CRYPTO_VMAC = lib.kernelEnable.
      CONFIG_CRYPTO_CMAC = lib.kernelEnable.
      CONFIG_CRYPTO_RSA = lib.kernelEnable.
      CONFIG_CRYPTO_DH = lib.kernelEnable.
      CONFIG_CRYPTO_ECC = lib.kernelEnable.
      CONFIG_CRYPTO_ECDH = lib.kernelEnable.
      CONFIG_CRYPTO_ECDSA = lib.kernelEnable.
      CONFIG_CRYPTO_CURVE25519 = lib.kernelEnable.
      CONFIG_CRYPTO_CURVE448 = lib.kernelEnable.
      CONFIG_CRYPTO_ED25519 = lib.kernelEnable.
      CONFIG_CRYPTO_ED448 = lib.kernelEnable.

      # Random number generation
      CONFIG_CRYPTO_RNG = lib.kernelEnable.
      CONFIG_CRYPTO_DRBG_MENU = lib.kernelEnable.
      CONFIG_CRYPTO_DRBG_HMAC = lib.kernelEnable.
      CONFIG_CRYPTO_DRBG_HASH = lib.kernelEnable.
      CONFIG_CRYPTO_DRBG_CTR = lib.kernelEnable.
      CONFIG_CRYPTO_DRBG = lib.kernelEnable.
      CONFIG_CRYPTO_JITTERENTROPY = lib.kernelEnable.
      CONFIG_CRYPTO_USER_API = lib.kernelEnable.
      CONFIG_CRYPTO_USER_API_HASH = lib.kernelEnable.
      CONFIG_CRYPTO_USER_API_SKCIPHER = lib.kernelEnable.
      CONFIG_CRYPTO_USER_API_RNG = lib.kernelEnable.
      CONFIG_CRYPTO_USER_API_AEAD = lib.kernelEnable.
      CONFIG_CRYPTO_HASH_INFO = lib.kernelEnable.
      CONFIG_CRYPTO_HW = lib.kernelEnable.
      CONFIG_CRYPTO_DEV_VIRTIO = lib.kernelEnable.
      CONFIG_CRYPTO_DEV_SP_PSP = lib.kernelDisable.
      CONFIG_CRYPTO_DEV_CCP = lib.kernelDisable.
      CONFIG_CRYPTO_DEV_QAT = lib.kernelDisable.
      CONFIG_CRYPTO_DEV_CHELSIO = lib.kernelDisable.
      CONFIG_CRYPTO_DEV_NITROX = lib.kernelDisable.
      CONFIG_CRYPTO_DEV_VIRTIO = lib.kernelEnable.

      # Virtualization
      CONFIG_KVM = lib.kernelEnable.
      CONFIG_KVM_INTEL = lib.kernelEnable.
      CONFIG_KVM_AMD = lib.kernelEnable.
      CONFIG_KVM_AMD_SEV = lib.kernelEnable.
      CONFIG_KVM_INTEL_TDX = lib.kernelEnable.
      CONFIG_VHOST_NET = lib.kernelEnable.
      CONFIG_VHOST_VSOCK = lib.kernelEnable.
      CONFIG_VHOST = lib.kernelEnable.
      CONFIG_VIRTIO = lib.kernelEnable.
      CONFIG_VIRTIO_PCI = lib.kernelEnable.
      CONFIG_VIRTIO_PCI_LEGACY = lib.kernelDisable.
      CONFIG_VIRTIO_BALLOON = lib.kernelEnable.
      CONFIG_VIRTIO_INPUT = lib.kernelEnable.
      CONFIG_VIRTIO_MMIO = lib.kernelEnable.
      CONFIG_VIRTIO_MMIO_CMDLINE_DEVICES = lib.kernelEnable.
      CONFIG_HYPERV = lib.kernelDisable.
      CONFIG_XEN = lib.kernelDisable.

      # IOMMU / DMA protection
      CONFIG_IOMMU_API = lib.kernelEnable.
      CONFIG_IOMMU_SUPPORT = lib.kernelEnable.
      CONFIG_IOMMU_IOVA = lib.kernelEnable.
      CONFIG_IOMMU_DMA = lib.kernelEnable.
      CONFIG_AMD_IOMMU = lib.kernelEnable.
      CONFIG_INTEL_IOMMU = lib.kernelEnable.
      CONFIG_INTEL_IOMMU_FLOPPY_WA = lib.kernelDisable.
      CONFIG_INTEL_IOMMU_SCALABLE_MODE_DEFAULT_ON = lib.kernelEnable.
      CONFIG_IRQ_REMAP = lib.kernelEnable.
      CONFIG_HYPERV_IOMMU = lib.kernelDisable.

      # VFIO
      CONFIG_VFIO = lib.kernelEnable.
      CONFIG_VFIO_PCI = lib.kernelEnable.
      CONFIG_VFIO_PCI_VGA = lib.kernelEnable.
      CONFIG_VFIO_PCI_MMAP = lib.kernelEnable.
      CONFIG_VFIO_PCI_INTX = lib.kernelEnable.
      CONFIG_VFIO_PCI_IGD = lib.kernelEnable.
      CONFIG_VFIO_MDEV = lib.kernelEnable.
      CONFIG_VFIO_IOMMU_TYPE1 = lib.kernelEnable.
      CONFIG_VFIO_VIRQFD = lib.kernelEnable.
      CONFIG_VFIO_NOIOMMU = lib.kernelDisable.

      # Security modules
      CONFIG_SECURITY = lib.kernelEnable.
      CONFIG_SECURITY_NETWORK = lib.kernelEnable.
      CONFIG_SECURITY_INFINIBAND = lib.kernelDisable.
      CONFIG_SECURITY_NETWORK_XFRM = lib.kernelEnable.
      CONFIG_SECURITY_PATH = lib.kernelEnable.
      CONFIG_INTEL_TXT = lib.kernelDisable.
      CONFIG_HAVE_HARDENED_USERCOPY_ALLOCATOR = lib.kernelEnable.
      CONFIG_HARDENED_USERCOPY = lib.kernelEnable.
      CONFIG_HARDENED_USERCOPY_FALLBACK = lib.kernelDisable.
      CONFIG_HARDENED_USERCOPY_PAGESPAN = lib.kernelEnable.
      CONFIG_FORTIFY_SOURCE = lib.kernelEnable.
      CONFIG_STATIC_USERMODEHELPER = lib.kernelEnable.
      CONFIG_STATIC_USERMODEHELPER_PATH = '""'.
      CONFIG_SECURITY_SELINUX = lib.kernelDisable.
      CONFIG_SECURITY_SMACK = lib.kernelDisable.
      CONFIG_SECURITY_TOMOYO = lib.kernelDisable.
      CONFIG_SECURITY_APPARMOR = lib.kernelEnable.
      CONFIG_SECURITY_APPARMOR_HASH = lib.kernelEnable.
      CONFIG_SECURITY_APPARMOR_HASH_DEFAULT = '""'.
      CONFIG_SECURITY_YAMA = lib.kernelEnable.
      CONFIG_SECURITY_SAFESETID = lib.kernelEnable.
      CONFIG_SECURITY_LOCKDOWN_LSM = lib.kernelEnable.
      CONFIG_SECURITY_LOCKDOWN_LSM_EARLY = lib.kernelEnable.
      CONFIG_LOCK_DOWN_KERNEL_FORCE_CONFIDENTIALITY = lib.kernelEnable.
      CONFIG_DEFAULT_SECURITY_DAC = lib.kernelDisable.
      CONFIG_DEFAULT_SECURITY_APPARMOR = lib.kernelEnable.
      CONFIG_DEFAULT_SECURITY = '"apparmor"'.

      # Auditing
      CONFIG_AUDIT = lib.kernelEnable.
      CONFIG_AUDITSYSCALL = lib.kernelEnable.
      CONFIG_AUDIT_WATCH = lib.kernelEnable.
      CONFIG_AUDIT_TREE = lib.kernelEnable.
      CONFIG_HAVE_ARCH_AUDITSYSCALL = lib.kernelEnable.

      # Debug (disable in production)
      CONFIG_DEBUG_KERNEL = lib.mkIf (securityLevel == "paranoid") lib.kernelEnable.
      CONFIG_DEBUG_INFO = lib.kernelDisable.
      CONFIG_DEBUG_INFO_DWARF5 = lib.kernelDisable.
      CONFIG_DEBUG_INFO_BTF = lib.kernelDisable.
      CONFIG_GDB_SCRIPTS = lib.kernelDisable.
      CONFIG_KGDB = lib.kernelDisable.
      CONFIG_KGDB_SERIAL_CONSOLE = lib.kernelDisable.
      CONFIG_KPROBES = lib.kernelDisable.
      CONFIG_KRETPROBES = lib.kernelDisable.
      CONFIG_UPROBES = lib.kernelDisable.
      CONFIG_OPTIMIZE_INLINING = lib.kernelEnable.
      CONFIG_DEBUG_FS = lib.kernelDisable.
      CONFIG_DEBUG_RODATA_TEST = lib.kernelDisable.
      CONFIG_DEBUG_SET_MODULE_RONX = lib.kernelEnable.
      CONFIG_DEBUG_WX = lib.kernelEnable.
      CONFIG_DEBUG_CREDENTIALS = lib.kernelEnable.
      CONFIG_DEBUG_LIST = lib.kernelEnable.
      CONFIG_DEBUG_PI_LIST = lib.kernelEnable.
      CONFIG_DEBUG_SG = lib.kernelEnable.
      CONFIG_DEBUG_NOTIFIERS = lib.kernelEnable.
      CONFIG_DEBUG_STACK_USAGE = lib.kernelEnable.
      CONFIG_DEBUG_OBJECTS = lib.kernelDisable.
      CONFIG_DEBUG_OBJECTS_SELFTEST = lib.kernelDisable.
      CONFIG_DEBUG_OBJECTS_FREE = lib.kernelDisable.
      CONFIG_DEBUG_OBJECTS_TIMERS = lib.kernelDisable.
      CONFIG_DEBUG_OBJECTS_WORK = lib.kernelDisable.
      CONFIG_DEBUG_OBJECTS_RCU_HEAD = lib.kernelDisable.
      CONFIG_DEBUG_OBJECTS_PERCPU_COUNTER = lib.kernelDisable.
      CONFIG_DEBUG_OBJECTS_ENABLE_DEFAULT = lib.kernelDisable.
      CONFIG_DEBUG_SLAB = lib.kernelDisable.
      CONFIG_DEBUG_SLAB_LEAK = lib.kernelDisable.
      CONFIG_DEBUG_KMEMLEAK = lib.kernelDisable.
      CONFIG_DEBUG_KMEMLEAK_TEST = lib.kernelDisable.
      CONFIG_DEBUG_STACKOVERFLOW = lib.kernelEnable.
      CONFIG_DEBUG_SHIRQ = lib.kernelEnable.
      CONFIG_DEBUG_TIMEKEEPING = lib.kernelDisable.
      CONFIG_DEBUG_PREEMPT = lib.kernelDisable.

      # Tracing (minimal)
      CONFIG_FTRACE = lib.kernelEnable.
      CONFIG_FUNCTION_TRACER = lib.kernelDisable.
      CONFIG_FUNCTION_GRAPH_TRACER = lib.kernelDisable.
      CONFIG_STACK_TRACER = lib.kernelDisable.
      CONFIG_BLK_DEV_IO_TRACE = lib.kernelDisable.
      CONFIG_KPROBE_EVENTS = lib.kernelDisable.
      CONFIG_UPROBE_EVENTS = lib.kernelDisable.
      CONFIG_BPF_EVENTS = lib.kernelDisable.
      CONFIG_DYNAMIC_EVENTS = lib.kernelDisable.
      CONFIG_PROBE_EVENTS = lib.kernelDisable.
      CONFIG_DYNAMIC_FTRACE = lib.kernelDisable.
      CONFIG_FUNCTION_PROFILER = lib.kernelDisable.
      CONFIG_FTRACE_MCOUNT_RECORD = lib.kernelDisable.
      CONFIG_FTRACE_SYSCALLS = lib.kernelEnable.
      CONFIG_TRACING_SUPPORT = lib.kernelEnable.
      CONFIG_RING_BUFFER = lib.kernelEnable.
      CONFIG_RING_BUFFER_ALLOW_SWAP = lib.kernelDisable.
      CONFIG_TRACING = lib.kernelEnable.
      CONFIG_GENERIC_TRACER = lib.kernelEnable.
      CONFIG_TRACING_MAX_GRAPH_DEPTH = 0.

      # Performance monitoring (restricted)
      CONFIG_PERF_EVENTS = lib.kernelEnable.
      CONFIG_PERF_EVENTS_INTEL_UNCORE = lib.kernelDisable.
      CONFIG_PERF_EVENTS_INTEL_RAPL = lib.kernelDisable.
      CONFIG_PERF_EVENTS_INTEL_CSTATE = lib.kernelDisable.
      CONFIG_PERF_EVENTS_AMD_POWER = lib.kernelDisable.
      CONFIG_PERF_EVENTS_X86 = lib.kernelEnable.

      # ACPI
      CONFIG_ACPI = lib.kernelEnable.
      CONFIG_ACPI_EC_DEBUGFS = lib.kernelDisable.
      CONFIG_ACPI_CUSTOM_DSDT = lib.kernelDisable.
      CONFIG_ACPI_TABLE_UPGRADE = lib.kernelEnable.
      CONFIG_ACPI_DEBUG = lib.kernelDisable.
      CONFIG_ACPI_HOTPLUG_MEMORY = lib.kernelEnable.
      CONFIG_ACPI_HOTPLUG_IOAPIC = lib.kernelEnable.
      CONFIG_ACPI_HOTPLUG_PCI = lib.kernelEnable.

      # Power management
      CONFIG_SUSPEND = lib.kernelDisable.
      CONFIG_HIBERNATION = lib.kernelDisable.
      CONFIG_PM_SLEEP = lib.kernelDisable.
      CONFIG_PM_AUTOSLEEP = lib.kernelDisable.
      CONFIG_PM_WAKELOCKS = lib.kernelEnable.
      CONFIG_PM_WAKELOCKS_LIMIT = 100.
      CONFIG_PM_WAKELOCKS_GC = lib.kernelEnable.
      CONFIG_PM = lib.kernelEnable.
      CONFIG_PM_CLK = lib.kernelEnable.
      CONFIG_PM_GENERIC_DOMAINS = lib.kernelEnable.
      CONFIG_PM_GENERIC_DOMAINS_SLEEP = lib.kernelDisable.
      CONFIG_PM_GENERIC_DOMAINS_OF = lib.kernelEnable.
      CONFIG_ENERGY_MODEL = lib.kernelEnable.
      CONFIG_ARCH_SUPPORTS_ACPI = lib.kernelEnable.
      CONFIG_ACPI_POWER_METER = lib.kernelDisable.

      # CPU frequency scaling
      CONFIG_CPU_FREQ = lib.kernelEnable.
      CONFIG_CPU_FREQ_GOV_ATTR_SET = lib.kernelEnable.
      CONFIG_CPU_FREQ_GOV_COMMON = lib.kernelEnable.
      CONFIG_CPU_FREQ_STAT = lib.kernelDisable.
      CONFIG_CPU_FREQ_DEFAULT_GOV_PERFORMANCE = lib.kernelEnable.
      CONFIG_CPU_FREQ_GOV_PERFORMANCE = lib.kernelEnable.
      CONFIG_CPU_FREQ_GOV_POWERSAVE = lib.kernelDisable.
      CONFIG_CPU_FREQ_GOV_USERSPACE = lib.kernelDisable.
      CONFIG_CPU_FREQ_GOV_ONDEMAND = lib.kernelDisable.
      CONFIG_CPU_FREQ_GOV_CONSERVATIVE = lib.kernelDisable.
      CONFIG_CPU_FREQ_GOV_SCHEDUTIL = lib.kernelEnable.

      # CPU idle
      CONFIG_CPU_IDLE = lib.kernelEnable.
      CONFIG_CPU_IDLE_GOV_LADDER = lib.kernelEnable.
      CONFIG_CPU_IDLE_GOV_MENU = lib.kernelEnable.
      CONFIG_CPU_IDLE_GOV_TEO = lib.kernelEnable.
      CONFIG_CPU_IDLE_GOV_HALTPOLL = lib.kernelEnable.

      # Intel/AMD specific
      CONFIG_X86_INTEL_PSTATE = lib.kernelEnable.
      CONFIG_X86_AMD_PSTATE = lib.kernelEnable.
      CONFIG_X86_ACPI_CPUFREQ = lib.kernelEnable.
      CONFIG_X86_POWERNOW_K8 = lib.kernelDisable.
      CONFIG_X86_SPEEDSTEP_LIB = lib.kernelDisable.
      CONFIG_X86_SPEEDSTEP_ICH = lib.kernelDisable.
      CONFIG_X86_SPEEDSTEP_SMI = lib.kernelDisable.
      CONFIG_X86_P4_CLOCKMOD = lib.kernelDisable.
      CONFIG_X86_LONGRUN = lib.kernelDisable.
      CONFIG_X86_LONGHAUL = lib.kernelDisable.
      CONFIG_X86_E_POWERSAVER = lib.kernelDisable.

      # Memory encryption
      CONFIG_AMD_MEM_ENCRYPT = lib.kernelEnable.
      CONFIG_AMD_MEM_ENCRYPT_ACTIVE_BY_DEFAULT = lib.kernelEnable.

      # TPM
      CONFIG_TCG_TPM = lib.kernelEnable.
      CONFIG_TCG_TIS = lib.kernelEnable.
      CONFIG_TCG_TIS_I2C_ATMEL = lib.kernelDisable.
      CONFIG_TCG_TIS_I2C_INFINEON = lib.kernelDisable.
      CONFIG_TCG_TIS_I2C_NUVOTON = lib.kernelDisable.
      CONFIG_TCG_NSC = lib.kernelDisable.
      CONFIG_TCG_ATMEL = lib.kernelDisable.
      CONFIG_TCG_INFINEON = lib.kernelDisable.
      CONFIG_TCG_CRB = lib.kernelEnable.
      CONFIG_TCG_VTPM_PROXY = lib.kernelEnable.
      CONFIG_TCG_TIS_ST33ZP24 = lib.kernelDisable.
      CONFIG_TCG_TIS_ST33ZP24_I2C = lib.kernelDisable.
      CONFIG_TCG_TIS_ST33ZP24_SPI = lib.kernelDisable.
      CONFIG_TPM_INTERFACE_SPEC = lib.kernelEnable.
      CONFIG_TCG_TIS_CORE = lib.kernelEnable.
      CONFIG_TCG_TIS_SPI = lib.kernelDisable.
      CONFIG_TCG_TIS_SPI_CR50 = lib.kernelDisable.
      CONFIG_TCG_TIS_I2C = lib.kernelDisable.

      # IMA/EVM
      CONFIG_IMA = lib.kernelEnable.
      CONFIG_IMA_MEASURE_PCR_IDX = 10.
      CONFIG_IMA_LSM_RULES = lib.kernelEnable.
      CONFIG_IMA_APPRAISE = lib.kernelEnable.
      CONFIG_IMA_APPRAISE_BOOTPARAM = lib.kernelEnable.
      CONFIG_IMA_READ_POLICY = lib.kernelEnable.
      CONFIG_IMA_WRITE_POLICY = lib.kernelEnable.
      CONFIG_IMA_ARCH_POLICY = lib.kernelEnable.
      CONFIG_IMA_DEFAULT_HASH = '"sha256"'.
      CONFIG_IMA_NG_TEMPLATE = lib.kernelEnable.
      CONFIG_IMA_SIG_TEMPLATE = lib.kernelEnable.
      CONFIG_IMA_KEYRINGS_PERMIT_SIGNED_BY_BUILTIN_OR_SECONDARY = lib.kernelEnable.
      CONFIG_IMA_BLACKLIST_KEYRING = lib.kernelEnable.
      CONFIG_IMA_X509_PATH = lib.kernelEnable.
      CONFIG_EVM = lib.kernelEnable.
      CONFIG_EVM_HMAC = lib.kernelEnable.
      CONFIG_EVM_ATTR_FSUUID = lib.kernelEnable.
      CONFIG_EVM_EXTRA_SMACK_XATTRS = lib.kernelEnable.
      CONFIG_EVM_LOAD_X509 = lib.kernelEnable.

      # Kernel signing
      CONFIG_MODULE_SIG = lib.kernelEnable.
      CONFIG_MODULE_SIG_FORCE = lib.kernelEnable.
      CONFIG_MODULE_SIG_ALL = lib.kernelEnable.
      CONFIG_MODULE_SIG_SHA512 = lib.kernelEnable.
      CONFIG_MODULE_SIG_KEY = '""'.
      CONFIG_SYSTEM_TRUSTED_KEYRING = lib.kernelEnable.
      CONFIG_SYSTEM_TRUSTED_KEYS = '""'.
      CONFIG_SYSTEM_EXTRA_CERTIFICATE = lib.kernelDisable.
      CONFIG_SYSTEM_EXTRA_CERTIFICATE_SIZE = 0.
      CONFIG_SECONDARY_TRUSTED_KEYRING = lib.kernelEnable.
      CONFIG_SYSTEM_BLACKLIST_KEYRING = lib.kernelEnable.
      CONFIG_SYSTEM_BLACKLIST_HASH_LIST = '""'.

      # Kernel page table isolation
      CONFIG_PAGE_TABLE_ISOLATION = lib.kernelEnable.

      # Randomize memory layout
      CONFIG_RANDOMIZE_BASE = lib.kernelEnable.
      CONFIG_RANDOMIZE_MEMORY = lib.kernelEnable.

      # Hardened allocator
      CONFIG_SLUB_DEBUG = lib.kernelDisable.
      CONFIG_SLUB_MEMCG_SYSFS_ON = lib.kernelDisable.
      CONFIG_SLUB_CPU_PARTIAL = lib.kernelDisable.
      CONFIG_HARDENED_USERCOPY = lib.kernelEnable.
      CONFIG_FORTIFY_SOURCE = lib.kernelEnable.

      # Refcount
      CONFIG_REFCOUNT_FULL = lib.kernelEnable.

      # Disable kernel debugging features that leak info
      CONFIG_DEBUG_INFO = lib.kernelDisable.
      CONFIG_DEBUG_INFO_DWARF5 = lib.kernelDisable.
      CONFIG_DEBUG_INFO_BTF = lib.kernelDisable.
      CONFIG_DEBUG_KERNEL = lib.kernelDisable.
      CONFIG_DEBUG_MISC = lib.kernelDisable.
    };

    # CIP kernel config (if using CIP kernel)
    cipConfig = {
      # CIP specific options would go here
    };

    # Architecture-specific
    archConfig = {
      x86_64 = {
        CONFIG_X86_INTEL_TDX_GUEST = lib.kernelEnable.
        CONFIG_X86_INTEL_TDX_HOST = lib.kernelEnable.
        CONFIG_X86_AMD_SEV = lib.kernelEnable.
        CONFIG_X86_AMD_SEV_ES = lib.kernelEnable.
        CONFIG_X86_AMD_SEV_SNP = lib.kernelEnable.
        CONFIG_KVM_INTEL_TDX = lib.kernelEnable.
        CONFIG_KVM_AMD_SEV = lib.kernelEnable.
        CONFIG_INTEL_IOMMU = lib.kernelEnable.
        CONFIG_INTEL_IOMMU_SCALABLE_MODE_DEFAULT_ON = lib.kernelEnable.
        CONFIG_INTEL_IOMMU_FLOPPY_WA = lib.kernelDisable.
        CONFIG_AMD_IOMMU = lib.kernelEnable.
        CONFIG_X86_MSR = lib.kernelEnable.
        CONFIG_X86_CPUID = lib.kernelEnable.
        CONFIG_X86_64 = lib.kernelEnable.
        CONFIG_X86_INTEL_PSTATE = lib.kernelEnable.
        CONFIG_X86_AMD_PSTATE = lib.kernelEnable.
      };
      aarch64 = {
        CONFIG_ARM64_PAN = lib.kernelEnable.
        CONFIG_ARM64_LSE = lib.kernelEnable.
        CONFIG_ARM64_SSBS = lib.kernelEnable.
        CONFIG_ARM64_BTI = lib.kernelEnable.
        CONFIG_ARM64_MTE = lib.kernelEnable.
        CONFIG_ARM64_SVE = lib.kernelEnable.
        CONFIG_ARM64_PTR_AUTH = lib.kernelEnable.
        CONFIG_KVM_ARM_HOST = lib.kernelEnable.
        CONFIG_KVM_ARM_PMU = lib.kernelEnable.
        CONFIG_KVM_ARM_VGIC_V3 = lib.kernelEnable.
        CONFIG_KVM_ARM_VGIC_ITS = lib.kernelEnable.
      };
    };

  in
  lib.optionalAttrs (cfg.platform == "x86_64") archConfig.x86_64
  // lib.optionalAttrs (cfg.platform == "aarch64") archConfig.aarch64
  // baseHardening
  // lib.optionalAttrs (securityLevel == "paranoid") {
    # Additional paranoid settings
    CONFIG_KASAN = lib.kernelEnable.
    CONFIG_KASAN_INLINE = lib.kernelEnable.
    CONFIG_KCSAN = lib.kernelEnable.
    CONFIG_DEBUG_KERNEL = lib.kernelEnable.
    CONFIG_DEBUG_VM = lib.kernelEnable.
    CONFIG_DEBUG_VM_RB = lib.kernelEnable.
    CONFIG_DEBUG_VM_PGFLAGS = lib.kernelEnable.
    CONFIG_DEBUG_VM_TRANSLATION = lib.kernelEnable.
    CONFIG_DEBUG_VM_LEAK = lib.kernelEnable.
    CONFIG_DEBUG_SLAB = lib.kernelEnable.
    CONFIG_DEBUG_SLAB_LEAK = lib.kernelEnable.
    CONFIG_DEBUG_KMEMLEAK = lib.kernelEnable.
    CONFIG_DEBUG_KMEMLEAK_TEST = lib.kernelEnable.
    CONFIG_DEBUG_KMEMLEAK_DEFAULT_OFF = lib.kernelDisable.
  }
  // lib.optionalAttrs (securityLevel == "standard") {
    # Standard settings (less aggressive)
    CONFIG_KASAN = lib.kernelDisable.
    CONFIG_KCSAN = lib.kernelDisable.
    CONFIG_DEBUG_KERNEL = lib.kernelDisable.
  };

  # Kernel modules to build
  boot.kernelModules = [
    "vfio"
    "vfio_pci"
    "vfio_iommu_type1"
    "vfio_mdev"
    "kvm"
    "kvm_intel"
    "kvm_amd"
    "kvm_intel_tdx"
    "kvm_amd_sev"
    "virtio"
    "virtio_pci"
    "virtio_balloon"
    "virtio_input"
    "virtio_mmio"
    "virtio_net"
    "virtio_blk"
    "virtio_scsi"
    "virtio_console"
    "virtio_rng"
    "virtio_gpu"
    "virtio_vsock"
    "vhost"
    "vhost_net"
    "vhost_vsock"
    "vhost_scsi"
    "tpm"
    "tpm_tis"
    "tpm_crb"
    "tpm_vtpm_proxy"
    "rng_core"
    "jitterentropy_rng"
    "intel_rng"
    "amd_rng"
  ];

  # Initrd modules
  boot.initrd.kernelModules = [
    "vfio"
    "vfio_pci"
    "vfio_iommu_type1"
    "kvm"
    "kvm_intel"
    "kvm_amd"
    "tpm"
    "tpm_tis"
    "tpm_crb"
  ];

  # Kernel signing (requires keys)
  boot.kernel.sign = true;
  # boot.kernel.signingKey = "/etc/sentinel/keys/kernel-signing.key";
  # boot.kernel.signingCert = "/etc/sentinel/keys/kernel-signing.crt";

  # AppArmor profiles
  security.apparmor = {
    enable = true;
    profiles = {
      # Default deny profile for untrusted binaries
      "sentinel-untrusted" = ''
        #include <tunables/global>
        profile sentinel-untrusted flags=(attach_disconnected) {
          #include <abstractions/base>
          #include <abstractions/nameservice>
          deny network,
          deny capability,
          deny mount,
          deny umount,
          deny ptrace,
          deny signal,
          file r,
          file w,
          file x,
        }
      '';
      # Network tools
      "sentinel-network-tool" = ''
        #include <tunables/global>
        profile sentinel-network-tool flags=(attach_disconnected) {
          #include <abstractions/base>
          #include <abstractions/nameservice>
          network inet raw,
          network inet6 raw,
          capability net_raw,
          capability net_admin,
          file r,
          file w,
        }
      '';
      # AI inference
      "sentinel-ai-inference" = ''
        #include <tunables/global>
        profile sentinel-ai-inference flags=(attach_disconnected) {
          #include <abstractions/base>
          file r,
          file w,
          /var/lib/sentinel/models/** r,
          /tmp/llama-* rw,
          capability sys_resource,
        }
      '';
    };
  };

  # Additional kernel packages for development
  boot.kernelPackages = pkgs.linuxPackages_hardened;
}