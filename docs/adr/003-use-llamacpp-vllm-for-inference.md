# 003 - Use llama.cpp and vLLM for Local AI Inference

## Status
Accepted

## Context
We need local AI inference for:
- LLM-driven tool orchestration (planning, DAG generation)
- Code generation and vulnerability analysis
- Threat intelligence (anomaly detection, explanation)
- Operator copilot (natural language interface)
- Report generation

Requirements:
- **Fully offline**: No external API calls; air-gapped operation
- **Model sovereignty**: User owns models and data
- **Hardware acceleration**: GPU (NVIDIA CUDA, AMD ROCm, Intel Xe), NPU
- **Quantization support**: 4-bit, 8-bit for consumer hardware
- **OpenAI-compatible API**: Ecosystem compatibility
- **Streaming & batching**: High throughput
- **Model registry**: Signed, verified model distribution
- **Resource scheduling**: Priority, preemption, GPU partitioning

## Decision
We will use **llama.cpp** as the primary CPU/GPU inference engine and **vLLM** as the high-throughput multi-GPU serving engine, with an **Ollama-compatible gateway** for unified access.

## Consequences

### Positive
- **llama.cpp**: 
  - Excellent CPU inference (AVX2, NEON, WASM SIMD)
  - Broad GPU support (CUDA, ROCm, Metal, Vulkan, SYCL)
  - GGUF format with quantization (Q4_K_M, Q8_0, etc.)
  - Low memory footprint, fast startup
  - Embeddable as library
  - Active development, large community

- **vLLM**:
  - PagedAttention for efficient KV cache
  - Continuous batching for high throughput
  - Tensor/pipeline parallelism for multi-GPU
  - OpenAI-compatible API
  - CUDA graphs, FlashAttention, FlashInfer
  - Production-ready (used by LMSYS, Anyscale)

- **Gateway pattern**:
  - Single endpoint for operators
  - Routes to optimal backend per model
  - Ollama API compatibility for tooling
  - Can add more backends (TGI, TensorRT-LLM) later

### Negative
- **Two stacks to maintain**: llama.cpp and vLLM have different configs, models
- **Model format divergence**: llama.cpp prefers GGUF; vLLM uses Safetensors/HF
- **Resource contention**: Both want GPU memory; requires scheduler coordination
- **vLLM Python dependency**: Heavier deployment; Python supply chain risk
- **llama.cpp server**: Less mature than vLLM for production serving

### Neutral
- **Quantization alignment**: Use Q4_K_M GGUF for llama.cpp; AWQ/GPTQ for vLLM
- **Model registry**: ORAS + cosign works for both formats
- **Scheduler**: Custom priority-based scheduler manages both backends

## Alternatives Considered

### Single Backend: llama.cpp Only
- **Pros**: Simpler, one stack, GGUF universal
- **Cons**: Lower throughput for batch workloads; no continuous batching; limited multi-GPU scaling

### Single Backend: vLLM Only
- **Pros**: Higher throughput, better multi-GPU, production-grade
- **Cons**: No CPU fallback; heavier; Python runtime; no GGUF support (yet)

### TGI (Text Generation Inference)
- **Pros**: Production-grade, OpenAI API, good RoCE support
- **Cons**: HuggingFace-controlled; less flexible quantization; heavier

### TensorRT-LLM
- **Pros**: Maximum NVIDIA GPU performance
- **Cons**: NVIDIA-only; proprietary; complex build; no CPU fallback

### ONNX Runtime / DirectML
- **Pros**: Cross-platform, hardware agnostic
- **Cons**: Less optimized for LLMs; limited model support; no continuous batching

### Ollama as Backend
- **Pros**: Simple, model management built-in
- **Cons**: Go-based; less control over scheduling; single-threaded model loading

## Related Decisions
- ADR 004: Use ORAS + cosign for model registry
- ADR 006: Use priority-based scheduler for AI workloads
- ADR 007: Use local embeddings (BGE-M3, Nomic) for threat intel

## References
- [llama.cpp](https://github.com/ggml-org/llama.cpp)
- [vLLM](https://github.com/vllm-project/vllm)
- [PagedAttention](https://arxiv.org/abs/2309.06180)
- [GGUF Specification](https://github.com/ggml-org/ggml/blob/master/docs/gguf.md)
- [ORAS](https://oras.land/)

---

## Metadata
- **Date**: 2026-01-15
- **Authors**: Sentinel OS Architecture Team
- **Reviewers**: [TBD]
- **Ticket**: #3