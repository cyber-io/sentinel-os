{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.sentinel.ai;
in
{
  options.sentinel.ai = {
    enable = mkEnableOption "AI/ML inference stack";
    gpuSupport = mkEnableOption "GPU acceleration for inference";

    # Model storage
    modelsPath = mkOption {
      type = types.path;
      default = "/var/lib/sentinel/models";
      description = "Path to store AI models";
    };

    # Model registry
    registry = {
      enable = mkEnableOption "Local model registry (OCI-compatible)";
      port = mkOption {
        type = types.int;
        default = 5000;
        description = "Registry port";
      };
      storage = mkOption {
        type = types.path;
        default = "/var/lib/sentinel/registry";
        description = "Registry storage path";
      };
    };

    # Inference servers
    inference = {
      # llama.cpp server
      llamacpp = {
        enable = mkEnableOption "llama.cpp inference server";
        port = mkOption {
          type = types.int;
          default = 8080;
          description = "llama.cpp server port";
        };
        nGpuLayers = mkOption {
          type = types.int;
          default = -1;
          description = "Number of GPU layers (-1 = all)";
        };
        ctxSize = mkOption {
          type = types.int;
          default = 8192;
          description = "Context size";
        };
        batchSize = mkOption {
          type = types.int;
          default = 512;
          description = "Batch size";
        };
        ubatchSize = mkOption {
          type = types.int;
          default = 512;
          description = "Micro-batch size";
        };
        flashAttn = mkEnableOption "Flash attention";
        mlock = mkEnableOption "Lock memory (mlock)";
        noMmap = mkEnableOption "Disable mmap";
        numa = mkEnableOption "NUMA awareness";
        # Model to serve (can be overridden per-model)
        defaultModel = mkOption {
          type = types.str;
          default = "";
          description = "Default model to load";
        };
      };

      # vLLM server
      vllm = {
        enable = mkEnableOption "vLLM inference server";
        port = mkOption {
          type = types.int;
          default = 8000;
          description = "vLLM server port";
        };
        host = mkOption {
          type = types.str;
          default = "0.0.0.0";
          description = "Bind address";
        };
        tensorParallelSize = mkOption {
          type = types.int;
          default = 1;
          description = "Tensor parallel size (multi-GPU)";
        };
        pipelineParallelSize = mkOption {
          type = types.int;
          default = 1;
          description = "Pipeline parallel size";
        };
        gpuMemoryUtilization = mkOption {
          type = types.float;
          default = 0.9;
          description = "GPU memory utilization fraction";
        };
        maxModelLen = mkOption {
          type = types.int;
          default = 8192;
          description = "Maximum model sequence length";
        };
        dtype = mkOption {
          type = types.enum [ "auto" "half" "float16" "bfloat16" "float32" ];
          default = "auto";
          description = "Data type";
        };
        kvCacheDtype = mkOption {
          type = types.enum [ "auto" "fp8" "fp8_e4m3" "fp8_e5m2" ];
          default = "auto";
          description = "KV cache dtype";
        };
        quantization = mkOption {
          type = types.enum [ "auto" "awq" "gptq" "marlin" "gguf" "bitsandbytes" "fp8" ];
          default = "auto";
          description = "Quantization method";
        };
        enforceEager = mkEnableOption "Enforce eager execution (no CUDA graphs)";
        disableCustomAllReduce = mkEnableOption "Disable custom all-reduce";
      };

      # Ollama-compatible API gateway
      gateway = {
        enable = mkEnableOption "Ollama-compatible API gateway";
        port = mkOption {
          type = types.int;
          default = 11434;
          description = "Gateway port";
        };
        upstreamLlamacpp = mkOption {
          type = types.str;
          default = "http://localhost:8080";
          description = "llama.cpp upstream";
        };
        upstreamVllm = mkOption {
          type = types.str;
          default = "http://localhost:8000";
          description = "vLLM upstream";
        };
      };
    };

    # Model zoo configuration
    models = {
      # Pre-configured models to download/serve
      default = mkOption {
        type = types.listOf (types.submodule ({ ... }: {
          name = mkOption { type = types.str; description = "Model name"; };
          source = mkOption { type = types.str; description = "HuggingFace repo or URL"; };
          filename = mkOption { type = types.str; default = ""; description = "GGUF filename"; };
          quantization = mkOption { type = types.str; default = "Q4_K_M"; description = "Quantization"; };
          backend = mkOption { type = types.enum [ "llamacpp" "vllm" "both" ]; default = "llamacpp"; description = "Inference backend"; };
          contextLength = mkOption { type = types.int; default = 8192; description = "Context length"; };
          gpuLayers = mkOption { type = types.int; default = -1; description = "GPU layers"; };
          description = mkOption { type = types.str; default = ""; description = "Model description"; };
          tags = mkOption { type = types.listOf types.str; default = [ ]; description = "Model tags"; };
          verify = mkOption { type = types.bool; default = true; description = "Verify signature"; };
        }));
        default = [
          {
            name = "llama-3.1-8b-instruct";
            source = "bartowski/Meta-Llama-3.1-8B-Instruct-GGUF";
            filename = "Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf";
            quantization = "Q4_K_M";
            backend = "both";
            contextLength = 8192;
            description = "General-purpose instruction model";
            tags = [ "orchestrator" "general" ];
          }
          {
            name = "codellama-13b-instruct";
            source = "codellama/CodeLlama-13b-Instruct-GGUF";
            filename = "codellama-13b-instruct.Q4_K_M.gguf";
            quantization = "Q4_K_M";
            backend = "both";
            contextLength = 16384;
            description = "Code generation and analysis";
            tags = [ "coder" "analyzer" "exploit-dev" ];
          }
          {
            name = "deepseek-coder-7b";
            source = "deepseek-ai/deepseek-coder-7b-instruct-GGUF";
            filename = "deepseek-coder-7b-instruct.Q4_K_M.gguf";
            quantization = "Q4_K_M";
            backend = "both";
            contextLength = 16384;
            description = "Code generation, vulnerability analysis";
            tags = [ "coder" "vuln-analysis" ];
          }
          {
            name = "bge-m3";
            source = "BAAI/bge-m3-GGUF";
            filename = "bge-m3-Q4_K_M.gguf";
            quantization = "Q4_K_M";
            backend = "llamacpp";
            contextLength = 8192;
            description = "Multilingual embedding model";
            tags = [ "embedder" "semantic-search" ];
          }
          {
            name = "nomic-embed-text";
            source = "nomic-ai/nomic-embed-text-v1.5-GGUF";
            filename = "nomic-embed-text-v1.5.Q4_K_M.gguf";
            quantization = "Q4_K_M";
            backend = "llamacpp";
            contextLength = 8192;
            description = "Long-context embedding model";
            tags = [ "embedder" "long-context" ];
          }
        ];
        description = "Default models to configure";
      };

      # Auto-download models on first use
      autoDownload = mkEnableOption "Auto-download models on first request";

      # Verification keys
      verificationKeys = mkOption {
        type = types.listOf types.path;
        default = [ ];
        description = "Public keys for model verification";
      };
    };

    # Resource scheduling
    scheduler = {
      enable = mkEnableOption "AI workload scheduler";
      # Priority classes
      priorities = mkOption {
        type = types.attrsOf (types.submodule ({ ... }: {
          weight = mkOption { type = types.int; default = 100; description = "Priority weight"; };
          preemptible = mkOption { type = types.bool; default = true; description = "Can be preempted"; };
          maxGpuMemory = mkOption { type = types.str; default = "80%"; description = "Max GPU memory"; };
        }));
        default = {
          critical = { weight = 1000; preemptible = false; maxGpuMemory = "90%"; };
          high = { weight = 100; preemptible = false; maxGpuMemory = "80%"; };
          normal = { weight = 10; preemptible = true; maxGpuMemory = "70%"; };
          low = { weight = 1; preemptible = true; maxGpuMemory = "50%"; };
          batch = { weight = 0; preemptible = true; maxGpuMemory = "50%"; };
        };
        description = "Priority classes for AI workloads";
      };
    };

    # Monitoring
    monitoring = {
      enable = mkEnableOption "AI inference monitoring";
      metricsPort = mkOption {
        type = types.int;
        default = 9090;
        description = "Metrics port";
      };
      logRequests = mkEnableOption "Log inference requests";
    };
  };

  config = mkIf cfg.enable {
    # Create directories
    systemd.tmpfiles.rules = [
      "d ${cfg.modelsPath} 0755 sentinel sentinel -"
      "d ${cfg.registry.storage} 0755 sentinel sentinel -"
      "d /run/sentinel/ai 0755 sentinel sentinel -"
      "d /var/log/sentinel/ai 0755 sentinel sentinel -"
    ];

    # User/group for AI services
    users.users.sentinel-ai = {
      isSystemUser = true;
      description = "Sentinel AI inference user";
      group = "sentinel-ai";
      home = "/var/lib/sentinel/ai";
    };

    users.groups.sentinel-ai = {
      gid = 990;
      members = [ "sentinel" ];
    };

    # GPU support
    mkIf cfg.gpuSupport {
      # NVIDIA
      hardware.nvidia = {
        modesetting.enable = true;
        powerManagement.enable = true;
        powerManagement.finegrained = true;
        open = false; # Use proprietary for CUDA
        nvidiaSettings = true;
        nvidiaPersistenced = true;
      };

      # AMD ROCm
      hardware.amdgpu = {
        enable = true;
        rocm.enable = true;
      };

      # Intel
      hardware.intel = {
        gpu = {
          enable = true;
          xe = true;
        };
      };

      # Container GPU access
      virtualisation.containerd.settings.plugins."io.containerd.grpc.v1.cri".containerd.runtimes.kata.options = {
        gpus = "all";
      };

      # nvidia-container-toolkit
      environment.systemPackages = with pkgs; [ nvidia-container-toolkit ];
    };

    # Model registry (OCI-compatible)
    mkIf cfg.registry.enable {
      environment.systemPackages = with pkgs; [ oras crane cosign ];

      systemd.services.sentinel-registry = {
        description = "Sentinel Model Registry (ORAS)";
        wantedBy = [ "multi-user.target" ];
        after = [ "network-online.target" ];
        serviceConfig = {
          ExecStart = "${pkgs.writeScriptBin "sentinel-registry" ''}
            #!${pkgs.runtimeShell}
            # ORAS-based registry for signed models
            mkdir -p ${cfg.registry.storage}
            exec ${pkgs.oras}/bin/oras serve \\
              --config /etc/sentinel/registry/config.yaml \\
              ${cfg.registry.storage}
          '';
          User = "sentinel-ai";
          Group = "sentinel-ai";
          StateDirectory = "sentinel-registry";
        };
      };

      # Registry config
      systemd.services.sentinel-registry-config = {
        description = "Sentinel Registry Configuration";
        wantedBy = [ "multi-user.target" ];
        before = [ "sentinel-registry.service" ];
        script = ''
          mkdir -p /etc/sentinel/registry
          cat > /etc/sentinel/registry/config.yaml << 'EOF'
    server:
      address: ":${toString cfg.registry.port}"
      tls:
        cert: "/etc/sentinel/keys/registry.crt"
        key: "/etc/sentinel/keys/registry.key"
    storage:
      filesystem:
        rootdirectory: "${cfg.registry.storage}"
    auth:
      disabled: true  # Use mutual TLS instead
    log:
      level: info
    EOF
        '';
      };
    };

    # llama.cpp server
    mkIf cfg.inference.llamacpp.enable {
      environment.systemPackages = with pkgs; [ llama-cpp ];

      systemd.services.sentinel-llamacpp = {
        description = "Sentinel llama.cpp Inference Server";
        wantedBy = [ "multi-user.target" ];
        after = [ "network-online.target" "sentinel-registry.service" ];
        requires = [ "sentinel-registry.service" ];
        serviceConfig = {
          ExecStart = "${pkgs.writeScriptBin "sentinel-llamacpp" ''}
            #!${pkgs.runtimeShell}
            MODEL_PATH="${cfg.modelsPath}/${cfg.inference.llamacpp.defaultModel}"
            if [ ! -f "$MODEL_PATH" ] && [ -n "${cfg.inference.llamacpp.defaultModel}" ]; then
              echo "Model not found: $MODEL_PATH"
              echo "Run: sentinel-model pull ${cfg.inference.llamacpp.defaultModel}"
              exit 1
            fi

            ARGS=(
              --host 0.0.0.0
              --port ${toString cfg.inference.llamacpp.port}
              --ctx-size ${toString cfg.inference.llamacpp.ctxSize}
              --batch-size ${toString cfg.inference.llamacpp.batchSize}
              --ubatch-size ${toString cfg.inference.llamacpp.ubatchSize}
              --n-gpu-layers ${toString cfg.inference.llamacpp.nGpuLayers}
              --mlock ${if cfg.inference.llamacpp.mlock then "true" else "false"}
              --no-mmap ${if cfg.inference.llamacpp.noMmap then "true" else "false"}
              --numa ${if cfg.inference.llamacpp.numa then "true" else "false"}
              ${if cfg.inference.llamacpp.flashAttn then "--flash-attn" else ""}
              ${if cfg.inference.llamacpp.defaultModel != "" then "--model $MODEL_PATH" else ""}
            )

            exec ${pkgs.llama-cpp}/bin/llama-server "\${ARGS[@]}"
          '';
          User = "sentinel-ai";
          Group = "sentinel-ai";
          SupplementaryGroups = [ "kvm" "render" "video" ];
          StateDirectory = "sentinel-ai";
          LogsDirectory = "sentinel/ai";
          # Resource limits
          MemoryLimit = "24G";
          CPUQuota = "200%";
          # GPU access
          DeviceAllow = "char-drm rw char-nvidiactl rw char-nvidia-uvm rw block-nvidia* rw";
        };
      };
    };

    # vLLM server
    mkIf cfg.inference.vllm.enable {
      # vLLM requires Python with CUDA
      environment.systemPackages = with pkgs; [
        (python312Packages.vllm.override { cudaSupport = cfg.gpuSupport; })
        python312Packages.huggingface-hub
        python312Packages.safetensors
      ];

      systemd.services.sentinel-vllm = {
        description = "Sentinel vLLM Inference Server";
        wantedBy = [ "multi-user.target" ];
        after = [ "network-online.target" ];
        serviceConfig = {
          ExecStart = "${pkgs.writeScriptBin "sentinel-vllm" ''}
            #!${pkgs.runtimeShell}
            export CUDA_VISIBLE_DEVICES=0
            export VLLM_WORKER_MULTIPROC_METHOD=spawn

            exec ${pkgs.python312Packages.vllm}/bin/vllm serve \\
              --host ${cfg.inference.vllm.host} \\
              --port ${toString cfg.inference.vllm.port} \\
              --tensor-parallel-size ${toString cfg.inference.vllm.tensorParallelSize} \\
              --pipeline-parallel-size ${toString cfg.inference.vllm.pipelineParallelSize} \\
              --gpu-memory-utilization ${toString cfg.inference.vllm.gpuMemoryUtilization} \\
              --max-model-len ${toString cfg.inference.vllm.maxModelLen} \\
              --dtype ${cfg.inference.vllm.dtype} \\
              --kv-cache-dtype ${cfg.inference.vllm.kvCacheDtype} \\
              --quantization ${cfg.inference.vllm.quantization} \\
              ${if cfg.inference.vllm.enforceEager then "--enforce-eager" else ""} \\
              ${if cfg.inference.vllm.disableCustomAllReduce then "--disable-custom-all-reduce" else ""} \\
              --model ${cfg.modelsPath}/vllm-model
          '';
          User = "sentinel-ai";
          Group = "sentinel-ai";
          SupplementaryGroups = [ "kvm" "render" "video" ];
          StateDirectory = "sentinel-ai";
          LogsDirectory = "sentinel/ai";
          MemoryLimit = "32G";
          CPUQuota = "400%";
          DeviceAllow = "char-drm rw char-nvidiactl rw char-nvidia-uvm rw block-nvidia* rw";
          # Environment
          Environment = "PYTHONUNBUFFERED=1 HF_HUB_ENABLE_HF_TRANSFER=1";
        };
      };
    };

    # Ollama-compatible gateway
    mkIf cfg.inference.gateway.enable {
      environment.systemPackages = with pkgs; [
        (writeScriptBin "sentinel-ai-gateway" ''
          #!${pkgs.runtimeShell}
          # Simple proxy that routes to llama.cpp or vLLM based on model
          # In production, use a proper gateway like ollama-proxy or custom
          exec ${pkgs.python312}/bin/python3 -c "
import asyncio, aiohttp, sys
from aiohttp import web

UPSTREAM_LLAMACPP = '${cfg.inference.gateway.upstreamLlamacpp}'
UPSTREAM_VLLM = '${cfg.inference.gateway.upstreamVllm}'

MODEL_ROUTING = {
    'llama': UPSTREAM_LLAMACPP,
    'codellama': UPSTREAM_LLAMACPP,
    'deepseek': UPSTREAM_LLAMACPP,
    'bge': UPSTREAM_LLAMACPP,
    'nomic': UPSTREAM_LLAMACPP,
    'mistral': UPSTREAM_VLLM,
    'mixtral': UPSTREAM_VLLM,
}

async def proxy(request):
    model = request.match_info.get('model', '')
    upstream = MODEL_ROUTING.get(model.split(':')[0], UPSTREAM_LLAMACPP)
    url = f'{upstream}{request.rel_url}'

    async with aiohttp.ClientSession() as session:
        async with session.request(
            request.method, url,
            headers={k: v for k, v in request.headers.items() if k.lower() != 'host'},
            data=await request.read() if request.can_read_body else None
        ) as resp:
            body = await resp.read()
            return web.Response(
                body=body,
                status=resp.status,
                headers=resp.headers
            )

app = web.Application()
app.router.add_route('*', '/{tail:.*}', proxy)
web.run_app(app, host='0.0.0.0', port=${toString cfg.inference.gateway.port})
        ''
      ];

      systemd.services.sentinel-ai-gateway = {
        description = "Sentinel AI Gateway (Ollama-compatible)";
        wantedBy = [ "multi-user.target" ];
        after = [ "sentinel-llamacpp.service" "sentinel-vllm.service" ];
        serviceConfig = {
          ExecStart = "${pkgs.sentinel-ai-gateway}/bin/sentinel-ai-gateway";
          User = "sentinel-ai";
          Group = "sentinel-ai";
          StateDirectory = "sentinel-ai";
          Restart = "on-failure";
        };
      };
    };

    # Model management CLI
    environment.systemPackages = with pkgs; [
      (writeScriptBin "sentinel-model" ''
        #!${pkgs.runtimeShell}
        set -euo pipefail

        MODELS_DIR="${cfg.modelsPath}"
        REGISTRY="localhost:${toString cfg.registry.port}"

        usage() {
          cat << 'EOF'
    Usage: sentinel-model <command> [options]

    Commands:
      list                     List available models
      pull <model>             Download a model
      remove <model>           Remove a model
      serve <model>            Start inference server for model
      verify <model>           Verify model signature
      info <model>             Show model info
      search <query>           Search model registry
      tag <model> <tag>        Tag a model
      push <model>             Push model to registry

    Examples:
      sentinel-model pull llama-3.1-8b-instruct
      sentinel-model serve llama-3.1-8b-instruct
      sentinel-model list
    EOF
        }

        if [ $# -eq 0 ]; then
          usage
          exit 1
        fi

        cmd="$1"
        shift

        case "$cmd" in
          list)
            echo "Models in $MODELS_DIR:"
            find "$MODELS_DIR" -name "*.gguf" -o -name "*.safetensors" -o -name "*.bin" 2>/dev/null | while read f; do
              echo "  $(basename "$f")"
            done
            ;;
          pull)
            model="$1"
            if [ -z "$model" ]; then
              echo "Usage: sentinel-model pull <model>"
              exit 1
            fi
            echo "Pulling $model from registry..."
            # Use oras to pull from registry
            ${pkgs.oras}/bin/oras pull "$REGISTRY/sentinel/$model" --output "$MODELS_DIR"
            ;;
          remove)
            model="$1"
            echo "Removing $model..."
            rm -f "$MODELS_DIR/$model"
            ;;
          serve)
            model="$1"
            if [ -z "$model" ]; then
              echo "Usage: sentinel-model serve <model>"
              exit 1
            fi
            echo "Starting inference server for $model..."
            systemctl --user start sentinel-llamacpp@$model
            ;;
          verify)
            model="$1"
            echo "Verifying $model..."
            ${pkgs.cosign}/bin/cosign verify-blob \
              --key /etc/sentinel/keys/model-verification.pub \
              --signature "$MODELS_DIR/$model.sig" \
              "$MODELS_DIR/$model"
            ;;
          info)
            model="$1"
            echo "Model info for $model:"
            # Use gguf-metadata or similar
            ;;
          search)
            query="$1"
            echo "Searching registry for: $query"
            ${pkgs.oras}/bin/oras repo tags "$REGISTRY/sentinel" | grep "$query"
            ;;
          tag)
            model="$1"
            tag="$2"
            echo "Tagging $model as $tag..."
            ${pkgs.oras}/bin/oras tag "$REGISTRY/sentinel/$model" "$tag"
            ;;
          push)
            model="$1"
            echo "Pushing $model to registry..."
            ${pkgs.oras}/bin/oras push "$REGISTRY/sentinel/$model" \
              --artifact-type "application/vnd.sentinel.model" \
              "$MODELS_DIR/$model"
            ;;
          *)
            echo "Unknown command: $cmd"
            usage
            exit 1
            ;;
        esac
      '')
    ];

    # Model downloader service (systemd template)
    systemd.services."sentinel-model-downloader@" = {
      description = "Sentinel Model Downloader for %i";
      wantedBy = [ "multi-user.target" ];
      serviceConfig = {
        ExecStart = "${pkgs.writeScriptBin "sentinel-model-downloader" ''}
          #!${pkgs.runtimeShell}
          MODEL="%i"
          MODELS_DIR="${cfg.modelsPath}"

          # In production, look up model config from registry or local config
          echo "Downloading $MODEL to $MODELS_DIR..."

          # Use huggingface-cli or oras
          if command -v ${pkgs.huggingface-hub}/bin/huggingface-cli >/dev/null; then
            ${pkgs.huggingface-hub}/bin/huggingface-cli download \\
              --repo-type model \\
              --local-dir "$MODELS_DIR/$MODEL" \\
              "$MODEL"
          fi

          # Verify if keys available
          if [ -f /etc/sentinel/keys/model-verification.pub ]; then
            ${pkgs.cosign}/bin/cosign verify-blob \\
              --key /etc/sentinel/keys/model-verification.pub \\
              --signature "$MODELS_DIR/$MODEL.sig" \\
              "$MODELS_DIR/$MODEL" || {
              echo "Verification failed!"
              exit 1
            }
          fi

          echo "Download complete: $MODEL"
        '';
        User = "sentinel-ai";
        Group = "sentinel-ai";
        StateDirectory = "sentinel-ai";
        # Run once, don't restart
        Restart = "no";
      };
    };

    # Monitoring
    mkIf cfg.monitoring.enable {
      # Prometheus metrics exporter for llama.cpp
      environment.systemPackages = with pkgs; [ prometheus ];

      systemd.services.sentinel-ai-metrics = {
        description = "Sentinel AI Metrics Exporter";
        wantedBy = [ "multi-user.target" ];
        after = [ "sentinel-llamacpp.service" "sentinel-vllm.service" ];
        serviceConfig = {
          ExecStart = "${pkgs.writeScriptBin "sentinel-ai-metrics" ''}
            #!${pkgs.runtimeShell}
            # Scrape metrics from inference servers and expose for Prometheus
            exec ${pkgs.python312}/bin/python3 -c "
import time, requests, json
from http.server import HTTPServer, BaseHTTPRequestHandler
from prometheus_client import Counter, Histogram, Gauge, generate_latest

REQUESTS = Counter('sentinel_ai_requests_total', 'Total requests', ['model', 'backend', 'status'])
LATENCY = Histogram('sentinel_ai_request_duration_seconds', 'Request latency', ['model', 'backend'])
GPU_MEM = Gauge('sentinel_ai_gpu_memory_bytes', 'GPU memory usage', ['device'])
LOAD = Gauge('sentinel_ai_model_loaded', 'Model loaded', ['model', 'backend'])

class MetricsHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == '/metrics':
            self.send_response(200)
            self.send_header('Content-Type', 'text/plain')
            self.end_headers()
            self.wfile.write(generate_latest())
        else:
            self.send_response(404)
            self.end_headers()

def scrape():
    # Scrape llama.cpp
    try:
        r = requests.get('http://localhost:8080/metrics', timeout=5)
        # Parse and re-export
    except:
        pass
    # Scrape vLLM
    try:
        r = requests.get('http://localhost:8000/metrics', timeout=5)
    except:
        pass

if __name__ == '__main__':
    import threading
    t = threading.Thread(target=lambda: [scrape() or time.sleep(10) for _ in iter(int, 1)], daemon=True)
    t.start()
    server = HTTPServer(('0.0.0.0', ${toString cfg.monitoring.metricsPort}), MetricsHandler)
    server.serve_forever()
        ";
        User = "sentinel-ai";
        Group = "sentinel-ai";
        StateDirectory = "sentinel-ai";
      };
    };

    # Default models setup
    systemd.services.sentinel-models-setup = {
      description = "Setup Default AI Models";
      wantedBy = [ "multi-user.target" ];
      after = [ "sentinel-registry.service" ];
      before = [ "sentinel-llamacpp.service" "sentinel-vllm.service" ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = "${pkgs.writeScriptBin "sentinel-models-setup" ''}
          #!${pkgs.runtimeShell}
          MODELS_DIR="${cfg.modelsPath}"
          mkdir -p "$MODELS_DIR"

          # Create model index
          cat > "$MODELS_DIR/index.json" << 'EOF'
    {
      "models": [
        ${concatStringsSep ",\n        " (builtins.map (m: ''
          {
            "name": "${m.name}",
            "source": "${m.source}",
            "filename": "${m.filename}",
            "quantization": "${m.quantization}",
            "backend": "${m.backend}",
            "contextLength": ${toString m.contextLength},
            "gpuLayers": ${toString m.gpuLayers},
            "description": "${m.description}",
            "tags": [${concatStringsSep ", " (builtins.map (t: '"${t}"') m.tags)}]
          }
        '') cfg.models.default)}
      ]
    }
    EOF

          echo "Model index created at $MODELS_DIR/index.json"
          echo "To download models, run: sentinel-model pull <model-name>"
        '';
        User = "sentinel-ai";
        Group = "sentinel-ai";
      };
    };
  };
}