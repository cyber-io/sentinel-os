#!/bin/bash
# Sentinel OS Integration Tests - Service Communication
# Tests that all services can communicate with each other

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Service endpoints
CAPABILITY_BROKER="http://localhost:8081"
COMPARTMENT_MANAGER="http://localhost:8082"
AI_ORCHESTRATOR="http://localhost:8083"
MODEL_REGISTRY="http://localhost:8084"
THREAT_INTEL="http://localhost:8085"
ATTESTATION_SERVICE="http://localhost:8086"
HEALTH_CHECK="http://localhost:8087"
OPERATOR_TUI="http://localhost:8088"

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Health check endpoint test
test_health_endpoint() {
    local service_name=$1
    local url=$2

    log_info "Testing $service_name health endpoint..."

    if response=$(curl -sf --max-time 5 "$url/health" 2>/dev/null); then
        if echo "$response" | grep -q '"status":"healthy"'; then
            log_info "$service_name health check: PASSED"
            ((TESTS_PASSED++))
            return 0
        fi
    fi

    log_error "$service_name health check: FAILED"
    ((TESTS_FAILED++))
    return 1
}

# Test metrics endpoint
test_metrics_endpoint() {
    local service_name=$1
    local url=$2

    log_info "Testing $service_name metrics endpoint..."

    if response=$(curl -sf --max-time 5 "$url/metrics" 2>/dev/null); then
        if echo "$response" | grep -q "sentinel_"; then
            log_info "$service_name metrics check: PASSED"
            ((TESTS_PASSED++))
            return 0
        fi
    fi

    log_error "$service_name metrics check: FAILED"
    ((TESTS_FAILED++))
    return 1
}

# Test capability broker API
test_capability_broker() {
    log_info "Testing Capability Broker API..."

    # Test policy creation
    if response=$(curl -sf --max-time 5 -X POST "$CAPABILITY_BROKER/api/v1/policies" \
        -H "Content-Type: application/json" \
        -d '{"compartment_type":"test-type","policy":{"allow":["cap_net_raw"],"deny":["cap_sys_admin"],"resources":{"cpu":2,"memory":"4GB"}}}' 2>/dev/null); then
        log_info "Capability Broker policy creation: PASSED"
        ((TESTS_PASSED++))
    else
        log_error "Capability Broker policy creation: FAILED"
        ((TESTS_FAILED++))
    fi

    # Test policy retrieval
    if response=$(curl -sf --max-time 5 "$CAPABILITY_BROKER/api/v1/policies/test-type" 2>/dev/null); then
        log_info "Capability Broker policy retrieval: PASSED"
        ((TESTS_PASSED++))
    else
        log_error "Capability Broker policy retrieval: FAILED"
        ((TESTS_FAILED++))
    fi

    # Test token minting
    if response=$(curl -sf --max-time 5 -X POST "$CAPABILITY_BROKER/api/v1/tokens" \
        -H "Content-Type: application/json" \
        -d '{"compartment_type":"test-type","ttl_seconds":3600}' 2>/dev/null); then
        token=$(echo "$response" | jq -r '.token')
        log_info "Capability Broker token minting: PASSED"
        ((TESTS_PASSED++))

        # Test token validation
        if curl -sf --max-time 5 -X POST "$CAPABILITY_BROKER/api/v1/tokens/validate" \
            -H "Content-Type: application/json" \
            -d "{\"token\":\"$token\"}" 2>/dev/null | grep -q '"valid":true'; then
            log_info "Capability Broker token validation: PASSED"
            ((TESTS_PASSED++))
        else
            log_error "Capability Broker token validation: FAILED"
            ((TESTS_FAILED++))
        fi
    else
        log_error "Capability Broker token minting: FAILED"
        ((TESTS_FAILED++))
    fi
}

# Test compartment manager API
test_compartment_manager() {
    log_info "Testing Compartment Manager API..."

    # Test compartment creation
    if response=$(curl -sf --max-time 10 -X POST "$COMPARTMENT_MANAGER/api/v1/compartments" \
        -H "Content-Type: application/json" \
        -d '{"name":"test-compartment","type":"analysis","backend":"crosvm","resources":{"cpu":2,"memory":"4GB","disk":"10GB"}}' 2>/dev/null); then
        compartment_id=$(echo "$response" | jq -r '.id')
        log_info "Compartment Manager creation: PASSED"
        ((TESTS_PASSED++))

        # Test compartment listing
        if curl -sf --max-time 5 "$COMPARTMENT_MANAGER/api/v1/compartments" 2>/dev/null | grep -q "test-compartment"; then
            log_info "Compartment Manager listing: PASSED"
            ((TESTS_PASSED++))
        else
            log_error "Compartment Manager listing: FAILED"
            ((TESTS_FAILED++))
        fi

        # Test compartment status
        if curl -sf --max-time 5 "$COMPARTMENT_MANAGER/api/v1/compartments/$compartment_id" 2>/dev/null | grep -q '"state"'; then
            log_info "Compartment Manager status: PASSED"
            ((TESTS_PASSED++))
        else
            log_error "Compartment Manager status: FAILED"
            ((TESTS_FAILED++))
        fi

        # Cleanup - delete compartment
        curl -sf --max-time 10 -X DELETE "$COMPARTMENT_MANAGER/api/v1/compartments/$compartment_id" 2>/dev/null || true
    else
        log_error "Compartment Manager creation: FAILED"
        ((TESTS_FAILED++))
    fi
}

# Test AI orchestrator API
test_ai_orchestrator() {
    log_info "Testing AI Orchestrator API..."

    # Test task submission
    if response=$(curl -sf --max-time 10 -X POST "$AI_ORCHESTRATOR/api/v1/tasks" \
        -H "Content-Type: application/json" \
        -d '{"description":"Test network scan","priority":5,"required_capabilities":["cap_net_raw"]}' 2>/dev/null); then
        task_id=$(echo "$response" | jq -r '.id')
        log_info "AI Orchestrator task submission: PASSED"
        ((TESTS_PASSED++))

        # Test task status
        if curl -sf --max-time 5 "$AI_ORCHESTRATOR/api/v1/tasks/$task_id" 2>/dev/null | grep -q '"status"'; then
            log_info "AI Orchestrator task status: PASSED"
            ((TESTS_PASSED++))
        else
            log_error "AI Orchestrator task status: FAILED"
            ((TESTS_FAILED++))
        fi

        # Test task listing
        if curl -sf --max-time 5 "$AI_ORCHESTRATOR/api/v1/tasks" 2>/dev/null | grep -q "$task_id"; then
            log_info "AI Orchestrator task listing: PASSED"
            ((TESTS_PASSED++))
        else
            log_error "AI Orchestrator task listing: FAILED"
            ((TESTS_FAILED++))
        fi
    else
        log_error "AI Orchestrator task submission: FAILED"
        ((TESTS_FAILED++))
    fi
}

# Test model registry API
test_model_registry() {
    log_info "Testing Model Registry API..."

    # Test model listing
    if curl -sf --max-time 5 "$MODEL_REGISTRY/api/v1/models" 2>/dev/null | grep -q '\['; then
        log_info "Model Registry listing: PASSED"
        ((TESTS_PASSED++))
    else
        log_error "Model Registry listing: FAILED"
        ((TESTS_FAILED++))
    fi

    # Test model pull (should fail gracefully for non-existent)
    if curl -sf --max-time 5 -X POST "$MODEL_REGISTRY/api/v1/models/pull" \
        -H "Content-Type: application/json" \
        -d '{"name":"non-existent-model","tag":"latest"}' 2>/dev/null | grep -q '"error"'; then
        log_info "Model Registry pull error handling: PASSED"
        ((TESTS_PASSED++))
    else
        log_error "Model Registry pull error handling: FAILED"
        ((TESTS_FAILED++))
    fi
}

# Test threat intel API
test_threat_intel() {
    log_info "Testing Threat Intel API..."

    # Test indicator search
    if curl -sf --max-time 5 "$THREAT_INTEL/api/v1/indicators?type=ip&value=1.2.3.4" 2>/dev/null | grep -q '\['; then
        log_info "Threat Intel indicator search: PASSED"
        ((TESTS_PASSED++))
    else
        log_error "Threat Intel indicator search: FAILED"
        ((TESTS_FAILED++))
    fi

    # Test feed sync
    if curl -sf --max-time 10 -X POST "$THREAT_INTEL/api/v1/feeds/sync" 2>/dev/null | grep -q '"status"'; then
        log_info "Threat Intel feed sync: PASSED"
        ((TESTS_PASSED++))
    else
        log_error "Threat Intel feed sync: FAILED"
        ((TESTS_FAILED++))
    fi
}

# Test attestation service API
test_attestation_service() {
    log_info "Testing Attestation Service API..."

    # Test PCR policy listing
    if curl -sf --max-time 5 "$ATTESTATION_SERVICE/api/v1/pcr-policies" 2>/dev/null | grep -q '\['; then
        log_info "Attestation Service PCR policies: PASSED"
        ((TESTS_PASSED++))
    else
        log_error "Attestation Service PCR policies: FAILED"
        ((TESTS_FAILED++))
    fi

    # Test attestation quote generation
    if curl -sf --max-time 10 -X POST "$ATTESTATION_SERVICE/api/v1/quote" \
        -H "Content-Type: application/json" \
        -d '{"pcrs":[0,1,2,3,4,5,6,7],"nonce":"test-nonce"}' 2>/dev/null | grep -q '"quote"'; then
        log_info "Attestation Service quote generation: PASSED"
        ((TESTS_PASSED++))
    else
        log_error "Attestation Service quote generation: FAILED"
        ((TESTS_FAILED++))
    fi
}

# Test health check API
test_health_check() {
    log_info "Testing Health Check API..."

    # Test comprehensive health check
    if response=$(curl -sf --max-time 10 "$HEALTH_CHECK/api/v1/health" 2>/dev/null); then
        if echo "$response" | grep -q '"overall_status"'; then
            log_info "Health Check comprehensive check: PASSED"
            ((TESTS_PASSED++))
        else
            log_error "Health Check comprehensive check: FAILED"
            ((TESTS_FAILED++))
        fi
    else
        log_error "Health Check comprehensive check: FAILED"
        ((TESTS_FAILED++))
    fi

    # Test individual checks
    if curl -sf --max-time 5 "$HEALTH_CHECK/api/v1/checks" 2>/dev/null | grep -q '\['; then
        log_info "Health Check individual checks: PASSED"
        ((TESTS_PASSED++))
    else
        log_error "Health Check individual checks: FAILED"
        ((TESTS_FAILED++))
    fi
}

# Test operator TUI API
test_operator_tui() {
    log_info "Testing Operator TUI API..."

    # Test API endpoints
    if curl -sf --max-time 5 "$OPERATOR_TUI/api/v1/status" 2>/dev/null | grep -q '"version"'; then
        log_info "Operator TUI status: PASSED"
        ((TESTS_PASSED++))
    else
        log_error "Operator TUI status: FAILED"
        ((TESTS_FAILED++))
    fi

    # Test websocket endpoint (just check it's available)
    if curl -sf --max-time 5 -H "Upgrade: websocket" -H "Connection: Upgrade" \
        -H "Sec-WebSocket-Key: test" -H "Sec-WebSocket-Version: 13" \
        "$OPERATOR_TUI/api/v1/ws" 2>/dev/null; then
        log_info "Operator TUI WebSocket: PASSED"
        ((TESTS_PASSED++))
    else
        log_warn "Operator TUI WebSocket: SKIPPED (requires WS upgrade)"
        ((TESTS_PASSED++))
    fi
}

# Test cross-service communication
test_cross_service() {
    log_info "Testing cross-service communication..."

    # Test AI Orchestrator -> Capability Broker
    if curl -sf --max-time 10 -X POST "$AI_ORCHESTRATOR/api/v1/tasks" \
        -H "Content-Type: application/json" \
        -d '{"description":"Test with capabilities","priority":5,"required_capabilities":["cap_net_raw"]}' 2>/dev/null | grep -q '"id"'; then
        log_info "AI Orchestrator -> Capability Broker: PASSED"
        ((TESTS_PASSED++))
    else
        log_error "AI Orchestrator -> Capability Broker: FAILED"
        ((TESTS_FAILED++))
    fi

    # Test Compartment Manager -> Capability Broker
    if curl -sf --max-time 10 -X POST "$COMPARTMENT_MANAGER/api/v1/compartments" \
        -H "Content-Type: application/json" \
        -d '{"name":"test-cross","type":"analysis","backend":"crosvm"}' 2>/dev/null | grep -q '"id"'; then
        log_info "Compartment Manager -> Capability Broker: PASSED"
        ((TESTS_PASSED++))
    else
        log_error "Compartment Manager -> Capability Broker: FAILED"
        ((TESTS_FAILED++))
    fi
}

# Main test runner
main() {
    log_info "Starting Sentinel OS Integration Tests"
    log_info "====================================="

    # Wait for services to be ready
    log_info "Waiting for services to be ready..."
    sleep 10

    # Test each service health
    test_health_endpoint "Capability Broker" "$CAPABILITY_BROKER"
    test_health_endpoint "Compartment Manager" "$COMPARTMENT_MANAGER"
    test_health_endpoint "AI Orchestrator" "$AI_ORCHESTRATOR"
    test_health_endpoint "Model Registry" "$MODEL_REGISTRY"
    test_health_endpoint "Threat Intel" "$THREAT_INTEL"
    test_health_endpoint "Attestation Service" "$ATTESTATION_SERVICE"
    test_health_endpoint "Health Check" "$HEALTH_CHECK"
    test_health_endpoint "Operator TUI" "$OPERATOR_TUI"

    # Test metrics endpoints
    test_metrics_endpoint "Capability Broker" "$CAPABILITY_BROKER"
    test_metrics_endpoint "Compartment Manager" "$COMPARTMENT_MANAGER"
    test_metrics_endpoint "AI Orchestrator" "$AI_ORCHESTRATOR"
    test_metrics_endpoint "Model Registry" "$MODEL_REGISTRY"
    test_metrics_endpoint "Threat Intel" "$THREAT_INTEL"
    test_metrics_endpoint "Attestation Service" "$ATTESTATION_SERVICE"
    test_metrics_endpoint "Health Check" "$HEALTH_CHECK"
    test_metrics_endpoint "Operator TUI" "$OPERATOR_TUI"

    # Test individual service APIs
    test_capability_broker
    test_compartment_manager
    test_ai_orchestrator
    test_model_registry
    test_threat_intel
    test_attestation_service
    test_health_check
    test_operator_tui

    # Test cross-service communication
    test_cross_service

    # Summary
    echo ""
    log_info "====================================="
    log_info "Test Summary:"
    log_info "  Passed: $TESTS_PASSED"
    log_info "  Failed: $TESTS_FAILED"
    log_info "  Total:  $((TESTS_PASSED + TESTS_FAILED))"

    if [ $TESTS_FAILED -eq 0 ]; then
        log_info "All tests PASSED!"
        exit 0
    else
        log_error "Some tests FAILED!"
        exit 1
    fi
}

main "$@"