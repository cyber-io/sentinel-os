# Sentinel OS Package Definitions
# Custom packages with security hardening

{ lib, stdenv, fetchFromGitHub, fetchurl, cmake, make, gcc, pkg-config, ... }:

let
  # Common build flags for hardening
  hardeningFlags = [
    "-fstack-protector-strong"
    "-fstack-clash-protection"
    "-fcf-protection=full"
    "-D_FORTIFY_SOURCE=3"
    "-fPIE"
    "-pie"
    "-Wl,-z,relro,-z,now,-z,noexecstack"
    "-Wformat=2"
    "-Wformat-security"
    "-Werror=format-security"
    "-fno-omit-frame-pointer"
    "-mno-sse"
    "-mno-avx"
  ];

  # Hardened stdenv
  hardenedStdenv = stdenv // {
    cc = stdenv.cc.override {
      extraFlags = hardeningFlags;
    };
    cxx = stdenv.cxx.override {
      extraFlags = hardeningFlags;
    };
  };
in
{
  # Hardened Linux Kernel
  linuxKernel = hardenedStdenv.mkDerivation {
    pname = "linux-hardened";
    version = "6.6.52"; # CIP SLTS
    src = fetchurl {
      url = "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.6.52.tar.xz";
      sha256 = "sha256-...";
    };
    # Kernel config would be applied via kernelConfig in NixOS module
    # This is just the package definition
    nativeBuildInputs = [ gcc cmake pkg-config ];
    # Additional hardening
    configureFlags = [ "--enable-hardening" ];
    # Sign kernel
    postInstall = ''
      # Sign kernel modules
      find $out/lib/modules -name "*.ko" -exec ${stdenv.cc}/bin/objcopy --add-signature=.sig {} \;
    '';
  };

  # Capability Broker
  capabilityBroker = hardenedStdenv.mkDerivation {
    pname = "sentinel-capability-broker";
    version = "0.1.0";
    src = ./.;
    nativeBuildInputs = [ gcc cmake pkg-config ];
    buildPhase = ''
      # Build Rust binary
      cargo build --release --target x86_64-unknown-linux-musl
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp target/x86_64-unknown-linux-musl/release/sentinel-capability-broker $out/bin/
    '';
  };

  # Compartment Manager
  compartmentManager = hardenedStdenv.mkDerivation {
    pname = "sentinel-compartment-manager";
    version = "0.1.0";
    src = ./.;
    nativeBuildInputs = [ gcc cmake pkg-config crosvm ];
    buildPhase = ''
      cargo build --release
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp target/release/sentinel-compartment-manager $out/bin/
    '';
  };

  # AI Gateway
  aiGateway = hardenedStdenv.mkDerivation {
    pname = "sentinel-ai-gateway";
    version = "0.1.0";
    src = ./.;
    nativeBuildInputs = [ python312 python312Packages.pip python312Packages.aiohttp ];
    installPhase = ''
      mkdir -p $out/bin
      cp sentinel_ai_gateway.py $out/bin/
      chmod +x $out/bin/sentinel_ai_gateway.py
    '';
  };

  # Model Manager
  modelManager = hardenedStdenv.mkDerivation {
    pname = "sentinel-model-manager";
    version = "0.1.0";
    src = ./.;
    nativeBuildInputs = [ python312 python312Packages.pip python312Packages.huggingface-hub python312Packages.oras python312Packages.cosign ];
    installPhase = ''
      mkdir -p $out/bin
      cp sentinel_model_manager.py $out/bin/
      chmod +x $out/bin/sentinel_model_manager.py
    '';
  };

  # Attestation
  attestation = hardenedStdenv.mkDerivation {
    pname = "sentinel-attestation";
    version = "0.1.0";
    src = ./.;
    nativeBuildInputs = [ gcc cmake pkg-config tpm2-tss openssl ];
    buildPhase = ''
      cargo build --release
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp target/release/sentinel-attestation $out/bin/
    '';
  };

  # Health Check
  healthCheck = hardenedStdenv.mkDerivation {
    pname = "sentinel-health-check";
    version = "0.1.0";
    src = ./.;
    installPhase = ''
      mkdir -p $out/bin
      cp sentinel-health-check.sh $out/bin/
      chmod +x $out/bin/sentinel-health-check.sh
    '';
  };

  # Hardened nmap
  hardenedNmap = hardenedStdenv.mkDerivation {
    pname = "nmap";
    version = "7.95";
    src = fetchurl {
      url = "https://nmap.org/dist/nmap-7.95.tar.bz2";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ pkg-config libpcap openssl lua ];
    configureFlags = [
      "--without-zenmap"
      "--disable-nmap-update"
      "--with-libpcap=${libpcap}"
      "--with-openssl=${openssl}"
      "--with-lua=${lua}"
    ];
    # Hardening
    CFLAGS = lib.concatStringsSep " " hardeningFlags;
    CXXFLAGS = lib.concatStringsSep " " hardeningFlags;
    LDFLAGS = "-Wl,-z,relro,-z,now";
  };

  # Hardened masscan
  hardenedMasscan = hardenedStdenv.mkDerivation {
    pname = "masscan";
    version = "1.3.2";
    src = fetchFromGitHub {
      owner = "robertdavidgraham";
      repo = "masscan";
      rev = "1.3.2";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ gcc make pkg-config libpcap ];
    buildPhase = "make -j${toString stdenv.buildCores}";
    installPhase = ''
      mkdir -p $out/bin
      cp bin/masscan $out/bin/
    '';
    CFLAGS = lib.concatStringsSep " " hardeningFlags;
    LDFLAGS = "-Wl,-z,relro,-z,now";
  };

  # Hardened hashcat
  hardenedHashcat = hardenedStdenv.mkDerivation {
    pname = "hashcat";
    version = "6.2.6";
    src = fetchurl {
      url = "https://hashcat.net/files/hashcat-6.2.6.tar.gz";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ gcc make pkg-config openssl opencl-headers ocl-icd ];
    buildPhase = "make -j${toString stdenv.buildCores}";
    installPhase = ''
      mkdir -p $out/bin
      cp hashcat $out/bin/
      mkdir -p $out/share/hashcat
      cp -r charsets kernels masks modules OpenCL $out/share/hashcat/
    '';
    CFLAGS = lib.concatStringsSep " " hardeningFlags;
    LDFLAGS = "-Wl,-z,relro,-z,now";
  };

  # Hardened john
  hardenedJohn = hardenedStdenv.mkDerivation {
    pname = "john";
    version = "1.9.0";
    src = fetchFromGitHub {
      owner = "openwall";
      repo = "john";
      rev = "bleeding-jumbo";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ gcc make pkg-config openssl openmpi ];
    buildPhase = ''
      cd src
      make -j${toString stdenv.buildCores} clean linux-x86-64-avx2
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp run/john $out/bin/
      mkdir -p $out/share/john
      cp -r run/*.chr run/*.conf run/*.rules $out/share/john/
    '';
    CFLAGS = lib.concatStringsSep " " hardeningFlags;
    LDFLAGS = "-Wl,-z,relro,-z,now";
  };

  # Hardened metasploit
  hardenedMetasploit = hardenedStdenv.mkDerivation {
    pname = "metasploit-framework";
    version = "6.3.0";
    src = fetchFromGitHub {
      owner = "rapid7";
      repo = "metasploit-framework";
      rev = "6.3.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ ruby bundler pkg-config postgresql libxml2 libxslt openssl zlib ];
    installPhase = ''
      bundle config set --local deployment 'true'
      bundle config set --local without 'development test'
      bundle install -j${toString stdenv.buildCores}
      mkdir -p $out
      cp -r . $out/
      # Fix shebangs
      find $out -name "*.rb" -exec sed -i '1s|.*|#!/usr/bin/env ruby|' {} \;
    '';
  };

  # Hardened Ghidra
  hardenedGhidra = hardenedStdenv.mkDerivation {
    pname = "ghidra";
    version = "11.1";
    src = fetchurl {
      url = "https://github.com/NationalSecurityAgency/ghidra/releases/download/Ghidra_11.1_build/ghidra_11.1_PUBLIC_20240130.zip";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ unzip jdk ];
    installPhase = ''
      mkdir -p $out
      cp -r ghidra_* $out/ghidra
      # Create wrapper
      cat > $out/bin/ghidra << 'EOF'
      #!/bin/sh
      exec $out/ghidra/ghidraRun "$@"
      EOF
      chmod +x $out/bin/ghidra
    '';
  };

  # Hardened radare2
  hardenedRadare2 = hardenedStdenv.mkDerivation {
    pname = "radare2";
    version = "5.9.0";
    src = fetchFromGitHub {
      owner = "radareorg";
      repo = "radare2";
      rev = "5.9.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ gcc make pkg-config openssl libzip capstone ];
    buildPhase = ''
      ./configure --prefix=$out --enable-hardening
      make -j${toString stdenv.buildCores}
    '';
    installPhase = "make install";
    CFLAGS = lib.concatStringsSep " " hardeningFlags;
    LDFLAGS = "-Wl,-z,relro,-z,now";
  };

  # Hardened volatility3
  hardenedVolatility3 = hardenedStdenv.mkDerivation {
    pname = "volatility3";
    version = "2.10.0";
    src = fetchFromGitHub {
      owner = "volatilityfoundation";
      repo = "volatility3";
      rev = "2.10.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ python312 python312Packages.pip python312Packages.setuptools ];
    installPhase = ''
      mkdir -p $out/lib/python3.12/site-packages
      cp -r volatility3 $out/lib/python3.12/site-packages/
      mkdir -p $out/bin
      cat > $out/bin/vol << 'EOF'
      #!/bin/sh
      exec python3 -m volatility3 "$@"
      EOF
      chmod +x $out/bin/vol
    '';
  };

  # llama.cpp with hardware acceleration
  llamaCpp = hardenedStdenv.mkDerivation {
    pname = "llama.cpp";
    version = "b4410"; # Specific commit for reproducibility
    src = fetchFromGitHub {
      owner = "ggml-org";
      repo = "llama.cpp";
      rev = "b4410";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ cmake gcc pkg-config ];
    # Enable hardware acceleration
    cmakeFlags = [
      "-DLLAMA_CUBLAS=ON"
      "-DLLAMA_CUDA=ON"
      "-DLLAMA_HIPBLAS=ON"
      "-DLLAMA_VULKAN=ON"
      "-DLLAMA_METAL=ON"
      "-DLLAMA_BLAS=ON"
      "-DLLAMA_BLAS_VENDOR=OpenBLAS"
      "-DCMAKE_BUILD_TYPE=Release"
    ];
    buildPhase = ''
      cmake -B build ${lib.concatStringsSep " " cmakeFlags}
      cmake --build build --config Release -j${toString stdenv.buildCores}
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp build/bin/llama-server $out/bin/
      cp build/bin/llama-cli $out/bin/
      cp build/bin/llama-embedding $out/bin/
      cp build/bin/llama-bench $out/bin/
      cp build/bin/llama-gguf* $out/bin/ 2>/dev/null || true
      # Libraries
      mkdir -p $out/lib
      cp build/libllama*.so $out/lib/ 2>/dev/null || true
      cp build/libggml*.so $out/lib/ 2>/dev/null || true
    '';
    CFLAGS = lib.concatStringsSep " " hardeningFlags;
    CXXFLAGS = lib.concatStringsSep " " hardeningFlags;
    LDFLAGS = "-Wl,-z,relro,-z,now";
  };

  # vLLM
  vllm = hardenedStdenv.mkDerivation {
    pname = "vllm";
    version = "0.5.0";
    src = fetchFromGitHub {
      owner = "vllm-project";
      repo = "vllm";
      rev = "v0.5.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ python312 python312Packages.pip python312Packages.setuptools python312Packages.wheel ];
    # Build with CUDA support
    buildPhase = ''
      export MAX_JOBS=${toString stdenv.buildCores}
      export VLLM_TARGET_DEVICES="cuda,hip"
      pip install --no-build-isolation -e .
    '';
    installPhase = ''
      mkdir -p $out
      cp -r . $out/
      # Create wrapper
      cat > $out/bin/vllm << 'EOF'
      #!/bin/sh
      export PYTHONPATH=$out:$PYTHONPATH
      exec python3 -m vllm.entrypoints.openai.api_server "$@"
      EOF
      chmod +x $out/bin/vllm
    '';
  };

  # llama-cpp-python
  llamaCppPython = hardenedStdenv.mkDerivation {
    pname = "llama-cpp-python";
    version = "0.2.0";
    src = fetchFromGitHub {
      owner = "abetlen";
      repo = "llama-cpp-python";
      rev = "v0.2.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ python312 python312Packages.pip python312Packages.setuptools cmake llamaCpp ];
    buildPhase = ''
      export CMAKE_ARGS="-DLLAMA_CUBLAS=ON -DLLAMA_CUDA=ON"
      pip install --no-build-isolation -e .
    '';
    installPhase = ''
      pip install --no-build-isolation --prefix=$out .
    '';
  };

  # PyTorch
  pytorch = hardenedStdenv.mkDerivation {
    pname = "pytorch";
    version = "2.3.0";
    src = fetchFromGitHub {
      owner = "pytorch";
      repo = "pytorch";
      rev = "v2.3.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ python312 python312Packages.pip python312Packages.setuptools cmake ninja ];
    buildPhase = ''
      export MAX_JOBS=${toString stdenv.buildCores}
      export USE_CUDA=1
      export USE_CUDNN=1
      export USE_MKLDNN=1
      export BUILD_TEST=0
      python3 setup.py build
    '';
    installPhase = ''
      python3 setup.py install --prefix=$out
    '';
  };

  # ONNX Runtime
  onnxruntime = hardenedStdenv.mkDerivation {
    pname = "onnxruntime";
    version = "1.18.0";
    src = fetchFromGitHub {
      owner = "microsoft";
      repo = "onnxruntime";
      rev = "v1.18.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ cmake gcc python312 python312Packages.pip ];
    buildPhase = ''
      ./build.sh --config Release --parallel --build_shared_lib --enable_pybind
    '';
    installPhase = ''
      mkdir -p $out/lib
      cp build/Linux/Release/*.so $out/lib/
      pip install --prefix=$out build/Linux/Release/dist/*.whl
    '';
  };

  # seL4 Toolchain
  sel4Tools = hardenedStdenv.mkDerivation {
    pname = "sel4-tools";
    version = "13.0.0";
    src = fetchFromGitHub {
      owner = "seL4";
      repo = "sel4-tools";
      rev = "13.0.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ gcc cmake python312 ];
    buildPhase = ''
      cmake -B build -DCMAKE_INSTALL_PREFIX=$out
      cmake --build build -j${toString stdenv.buildCores}
    '';
    installPhase = "cmake --install build";
  };

  # crosvm
  crosvm = hardenedStdenv.mkDerivation {
    pname = "crosvm";
    version = "0.1.0"; # Use specific commit
    src = fetchFromGitHub {
      owner = "google";
      repo = "crosvm";
      rev = "main"; # Pin to specific commit in practice
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ rustc cargo pkg-config libcap libseccomp libglib openssl ];
    buildPhase = ''
      cargo build --release --features "gpu xen" --target x86_64-unknown-linux-musl
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp target/x86_64-unknown-linux-musl/release/crosvm $out/bin/
    '';
  };

  # Heads firmware
  heads = hardenedStdenv.mkDerivation {
    pname = "heads";
    version = "0.1.0";
    src = fetchFromGitHub {
      owner = "heads";
      repo = "heads";
      rev = "main"; # Pin to specific tag
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ gcc make pkg-config coreboot-utils ];
    buildPhase = ''
      make BOARD=qemu_x86_64 -j${toString stdenv.buildCores}
    '';
    installPhase = ''
      mkdir -p $out/share/heads
      cp build/heads.rom $out/share/heads/
    '';
  };

  # TrenchBoot
  trenchboot = hardenedStdenv.mkDerivation {
    pname = "trenchboot";
    version = "0.1.0";
    src = fetchFromGitHub {
      owner = "TrenchBoot";
      repo = "trenchboot";
      rev = "main"; # Pin to specific tag
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ gcc make pkg-config tpm2-tss ];
    buildPhase = "make -j${toString stdenv.buildCores}";
    installPhase = ''
      mkdir -p $out/bin
      cp tb_polgen $out/bin/
      cp tb_launch $out/bin/
    '';
  };

  # sbctl
  sbctl = hardenedStdenv.mkDerivation {
    pname = "sbctl";
    version = "0.10.0";
    src = fetchFromGitHub {
      owner = "Foxboron";
      repo = "sbctl";
      rev = "0.10.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ rustc cargo pkg-config openssl ];
    buildPhase = ''
      cargo build --release --target x86_64-unknown-linux-musl
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp target/x86_64-unknown-linux-musl/release/sbctl $out/bin/
    '';
  };

  # cosign
  cosign = hardenedStdenv.mkDerivation {
    pname = "cosign";
    version = "2.2.0";
    src = fetchFromGitHub {
      owner = "sigstore";
      repo = "cosign";
      rev = "v2.2.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ go ];
    buildPhase = ''
      go build -ldflags="-s -w" -o cosign ./cmd/cosign
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp cosign $out/bin/
    '';
  };

  # oras
  oras = hardenedStdenv.mkDerivation {
    pname = "oras";
    version = "1.1.0";
    src = fetchFromGitHub {
      owner = "oras-project";
      repo = "oras";
      rev = "v1.1.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ go ];
    buildPhase = ''
      go build -ldflags="-s -w" -o oras ./cmd/oras
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp oras $out/bin/
    '';
  };

  # crane
  crane = hardenedStdenv.mkDerivation {
    pname = "crane";
    version = "0.18.0";
    src = fetchFromGitHub {
      owner = "google";
      repo = "crane";
      rev = "v0.18.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ go ];
    buildPhase = ''
      go build -ldflags="-s -w" -o crane ./cmd/crane
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp crane $out/bin/
    '';
  };

  # syft
  syft = hardenedStdenv.mkDerivation {
    pname = "syft";
    version = "0.100.0";
    src = fetchFromGitHub {
      owner = "anchore";
      repo = "syft";
      rev = "v0.100.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ go ];
    buildPhase = ''
      go build -ldflags="-s -w" -o syft ./cmd/syft
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp syft $out/bin/
    '';
  };

  # grype
  grype = hardenedStdenv.mkDerivation {
    pname = "grype";
    version = "0.75.0";
    src = fetchFromGitHub {
      owner = "anchore";
      repo = "grype";
      rev = "v0.75.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ go ];
    buildPhase = ''
      go build -ldflags="-s -w" -o grype ./cmd/grype
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp grype $out/bin/
    '';
  };

  # trivy
  trivy = hardenedStdenv.mkDerivation {
    pname = "trivy";
    version = "0.50.0";
    src = fetchFromGitHub {
      owner = "aquasecurity";
      repo = "trivy";
      rev = "v0.50.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ go ];
    buildPhase = ''
      go build -ldflags="-s -w" -o trivy ./cmd/trivy
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp trivy $out/bin/
    '';
  };

  # prometheus node exporter
  prometheusNodeExporter = hardenedStdenv.mkDerivation {
    pname = "prometheus-node-exporter";
    version = "1.7.0";
    src = fetchFromGitHub {
      owner = "prometheus";
      repo = "node_exporter";
      rev = "v1.7.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ go ];
    buildPhase = ''
      go build -ldflags="-s -w" -o node_exporter .
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp node_exporter $out/bin/
    '';
  };

  # Vector
  vector = hardenedStdenv.mkDerivation {
    pname = "vector";
    version = "0.38.0";
    src = fetchFromGitHub {
      owner = "vectorizedio";
      repo = "vector";
      rev = "v0.38.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ rustc cargo ];
    buildPhase = ''
      cargo build --release --target x86_64-unknown-linux-musl
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp target/x86_64-unknown-linux-musl/release/vector $out/bin/
    '';
  };

  # nil
  nil = hardenedStdenv.mkDerivation {
    pname = "nil";
    version = "0.7.0";
    src = fetchFromGitHub {
      owner = "oxalica";
      repo = "nil";
      rev = "0.7.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ rustc cargo ];
    buildPhase = ''
      cargo build --release
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp target/release/nil $out/bin/
    '';
  };

  # nixpkgs-fmt
  nixpkgsFmt = hardenedStdenv.mkDerivation {
    pname = "nixpkgs-fmt";
    version = "0.5.0";
    src = fetchFromGitHub {
      owner = "nix-community";
      repo = "nixpkgs-fmt";
      rev = "0.5.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ rustc cargo ];
    buildPhase = ''
      cargo build --release
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp target/release/nixpkgs-fmt $out/bin/
    '';
  };

  # statix
  statix = hardenedStdenv.mkDerivation {
    pname = "statix";
    version = "0.10.0";
    src = fetchFromGitHub {
      owner = "nerdypepper";
      repo = "statix";
      rev = "0.10.0";
      sha256 = "sha256-...";
    };
    nativeBuildInputs = [ rustc cargo ];
    buildPhase = ''
      cargo build --release
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp target/release/statix $out/bin/
    '';
  };
}