#!/bin/bash
# Sentinel OS Contract Tests - API Compatibility
# Tests that service APIs conform to expected contracts

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Service endpoints
CAPABILITY_BROKER="http://localhost:8081"
COMPARTMENT_MANAGER="http://localhost:8082"
AI_ORCHESTRATOR="http://localhost:8083"
MODEL_REGISTRY="http://localhost:8084"
THREAT_INTEL="http://localhost:8085"
ATTESTATION_SERVICE="http://localhost:8086"
HEALTH_CHECK="http://localhost:8087"
OPERATOR_TUI="http://localhost:8088"

TESTS_PASSED=0
TESTS_FAILED=0

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }

# Validate JSON response has required fields
validate_json_fields() {
    local response=$1
    local required_fields=$2
    local service=$3
    local endpoint=$4

    for field in $required_fields; do
        if ! echo "$response" | jq -e ".$field" >/dev/null 2>&1; then
            log_error "$service $endpoint: Missing required field '$field'"
            return 1
        fi
    done
    return 0
}

# Test Capability Broker contracts
test_capability_broker_contracts() {
    log_info "Testing Capability Broker API contracts..."

    # Health contract
    response=$(curl -sf --max-time 5 "$CAPABILITY_BROKER/health")
    validate_json_fields "$response" "status timestamp version" "Capability Broker" "/health" || ((TESTS_FAILED++))
    if echo "$response" | jq -e '.status == "healthy"' >/dev/null; then
        ((TESTS_PASSED++))
    else
        log_error "Capability Broker /health: status not healthy"
        ((TESTS_FAILED++))
    fi

    # Policy contract
    response=$(curl -sf --max-time 5 -X POST "$CAPABILITY_BROKER/api/v1/policies" \
        -H "Content-Type: application/json" \
        -d '{"compartment_type":"contract-test","policy":{"allow":["cap_net_raw"],"deny":[],"resources":{"cpu":1,"memory":"1GB"}}}')
    validate_json_fields "$response" "id compartment_type policy version created_at" "Capability Broker" "POST /api/v1/policies" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    response=$(curl -sf --max-time 5 "$CAPABILITY_BROKER/api/v1/policies/contract-test")
    validate_json_fields "$response" "id compartment_type policy version created_at updated_at" "Capability Broker" "GET /api/v1/policies/{type}" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    # Token contract
    response=$(curl -sf --max-time 5 -X POST "$CAPABILITY_BROKER/api/v1/tokens" \
        -H "Content-Type: application/json" \
        -d '{"compartment_type":"contract-test","ttl_seconds":3600}')
    validate_json_fields "$response" "token expires_at capabilities" "Capability Broker" "POST /api/v1/tokens" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    token=$(echo "$response" | jq -r '.token')
    response=$(curl -sf --max-time 5 -X POST "$CAPABILITY_BROKER/api/v1/tokens/validate" \
        -H "Content-Type: application/json" \
        -d "{\"token\":\"$token\"}")
    validate_json_fields "$response" "valid capabilities expires_at" "Capability Broker" "POST /api/v1/tokens/validate" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    # Metrics contract
    response=$(curl -sf --max-time 5 "$CAPABILITY_BROKER/metrics")
    if echo "$response" | grep -q "sentinel_capability_broker_"; then
        ((TESTS_PASSED++))
    else
        log_error "Capability Broker /metrics: missing sentinel_capability_broker_ prefix"
        ((TESTS_FAILED++))
    fi
}

# Test Compartment Manager contracts
test_compartment_manager_contracts() {
    log_info "Testing Compartment Manager API contracts..."

    # Health contract
    response=$(curl -sf --max-time 5 "$COMPARTMENT_MANAGER/health")
    validate_json_fields "$response" "status timestamp version" "Compartment Manager" "/health" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    # Create compartment contract
    response=$(curl -sf --max-time 10 -X POST "$COMPARTMENT_MANAGER/api/v1/compartments" \
        -H "Content-Type: application/json" \
        -d '{"name":"contract-test","type":"analysis","backend":"crosvm","resources":{"cpu":2,"memory":"4GB","disk":"10GB"}}')
    validate_json_fields "$response" "id name type backend state resources created_at" "Compartment Manager" "POST /api/v1/compartments" || ((TESTS_FAILED++))
    compartment_id=$(echo "$response" | jq -r '.id')
    ((TESTS_PASSED++))

    # Get compartment contract
    response=$(curl -sf --max-time 5 "$COMPARTMENT_MANAGER/api/v1/compartments/$compartment_id")
    validate_json_fields "$response" "id name type backend state pid ip_address resources capabilities created_at started_at" "Compartment Manager" "GET /api/v1/compartments/{id}" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    # List compartments contract
    response=$(curl -sf --max-time 5 "$COMPARTMENT_MANAGER/api/v1/compartments")
    if echo "$response" | jq -e 'type == "array"' >/dev/null; then
        ((TESTS_PASSED++))
    else
        log_error "Compartment Manager GET /api/v1/compartments: response not array"
        ((TESTS_FAILED++))
    fi

    # Metrics contract
    response=$(curl -sf --max-time 5 "$COMPARTMENT_MANAGER/metrics")
    if echo "$response" | grep -q "sentinel_compartment_"; then
        ((TESTS_PASSED++))
    else
        log_error "Compartment Manager /metrics: missing sentinel_compartment_ prefix"
        ((TESTS_FAILED++))
    fi

    # Cleanup
    curl -sf --max-time 10 -X DELETE "$COMPARTMENT_MANAGER/api/v1/compartments/$compartment_id" 2>/dev/null || true
}

# Test AI Orchestrator contracts
test_ai_orchestrator_contracts() {
    log_info "Testing AI Orchestrator API contracts..."

    # Health contract
    response=$(curl -sf --max-time 5 "$AI_ORCHESTRATOR/health")
    validate_json_fields "$response" "status timestamp version" "AI Orchestrator" "/health" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    # Create task contract
    response=$(curl -sf --max-time 10 -X POST "$AI_ORCHESTRATOR/api/v1/tasks" \
        -H "Content-Type: application/json" \
        -d '{"description":"Contract test task","priority":3,"required_capabilities":["cap_dac_read_search"]}')
    validate_json_fields "$response" "id description context priority required_capabilities status created_at" "AI Orchestrator" "POST /api/v1/tasks" || ((TESTS_FAILED++))
    task_id=$(echo "$response" | jq -r '.id')
    ((TESTS_PASSED++))

    # Get task contract
    response=$(curl -sf --max-time 5 "$AI_ORCHESTRATOR/api/v1/tasks/$task_id")
    validate_json_fields "$response" "id description context priority required_capabilities target_compartment status plan current_step result error retry_count created_at updated_at started_at completed_at" "AI Orchestrator" "GET /api/v1/tasks/{id}" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    # List tasks contract
    response=$(curl -sf --max-time 5 "$AI_ORCHESTRATOR/api/v1/tasks")
    if echo "$response" | jq -e 'type == "array"' >/dev/null; then
        ((TESTS_PASSED++))
    else
        log_error "AI Orchestrator GET /api/v1/tasks: response not array"
        ((TESTS_FAILED++))
    fi

    # Metrics contract
    response=$(curl -sf --max-time 5 "$AI_ORCHESTRATOR/metrics")
    if echo "$response" | grep -q "sentinel_orchestrator_"; then
        ((TESTS_PASSED++))
    else
        log_error "AI Orchestrator /metrics: missing sentinel_orchestrator_ prefix"
        ((TESTS_FAILED++))
    fi
}

# Test Model Registry contracts
test_model_registry_contracts() {
    log_info "Testing Model Registry API contracts..."

    # Health contract
    response=$(curl -sf --max-time 5 "$MODEL_REGISTRY/health")
    validate_json_fields "$response" "status timestamp version" "Model Registry" "/health" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    # List models contract
    response=$(curl -sf --max-time 5 "$MODEL_REGISTRY/api/v1/models")
    if echo "$response" | jq -e 'type == "array"' >/dev/null; then
        ((TESTS_PASSED++))
    else
        log_error "Model Registry GET /api/v1/models: response not array"
        ((TESTS_FAILED++))
    fi

    # Get model contract (should 404 for non-existent)
    response=$(curl -sf --max-time 5 "$MODEL_REGISTRY/api/v1/models/non-existent-model:latest" 2>/dev/null || echo '{"error":"not found"}')
    if echo "$response" | jq -e '.error' >/dev/null; then
        ((TESTS_PASSED++))
    else
        log_error "Model Registry GET /api/v1/models/{name}: should return error for non-existent"
        ((TESTS_FAILED++))
    fi

    # Metrics contract
    response=$(curl -sf --max-time 5 "$MODEL_REGISTRY/metrics")
    if echo "$response" | grep -q "sentinel_model_registry_"; then
        ((TESTS_PASSED++))
    else
        log_error "Model Registry /metrics: missing sentinel_model_registry_ prefix"
        ((TESTS_FAILED++))
    fi
}

# Test Threat Intel contracts
test_threat_intel_contracts() {
    log_info "Testing Threat Intel API contracts..."

    # Health contract
    response=$(curl -sf --max-time 5 "$THREAT_INTEL/health")
    validate_json_fields "$response" "status timestamp version" "Threat Intel" "/health" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    # Search indicators contract
    response=$(curl -sf --max-time 5 "$THREAT_INTEL/api/v1/indicators?type=ip&value=8.8.8.8")
    if echo "$response" | jq -e 'type == "array"' >/dev/null; then
        ((TESTS_PASSED++))
    else
        log_error "Threat Intel GET /api/v1/indicators: response not array"
        ((TESTS_FAILED++))
    fi

    # Feed sync contract
    response=$(curl -sf --max-time 10 -X POST "$THREAT_INTEL/api/v1/feeds/sync")
    validate_json_fields "$response" "status synced_at feeds_synced indicators_added" "Threat Intel" "POST /api/v1/feeds/sync" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    # Metrics contract
    response=$(curl -sf --max-time 5 "$THREAT_INTEL/metrics")
    if echo "$response" | grep -q "sentinel_threat_intel_"; then
        ((TESTS_PASSED++))
    else
        log_error "Threat Intel /metrics: missing sentinel_threat_intel_ prefix"
        ((TESTS_FAILED++))
    fi
}

# Test Attestation Service contracts
test_attestation_service_contracts() {
    log_info "Testing Attestation Service API contracts..."

    # Health contract
    response=$(curl -sf --max-time 5 "$ATTESTATION_SERVICE/health")
    validate_json_fields "$response" "status timestamp version" "Attestation Service" "/health" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    # PCR policies contract
    response=$(curl -sf --max-time 5 "$ATTESTATION_SERVICE/api/v1/pcr-policies")
    if echo "$response" | jq -e 'type == "array"' >/dev/null; then
        ((TESTS_PASSED++))
    else
        log_error "Attestation Service GET /api/v1/pcr-policies: response not array"
        ((TESTS_FAILED++))
    fi

    # Quote contract
    response=$(curl -sf --max-time 10 -X POST "$ATTESTATION_SERVICE/api/v1/quote" \
        -H "Content-Type: application/json" \
        -d '{"pcrs":[0,1,2,3,4,5,6,7],"nonce":"contract-test-nonce"}')
    validate_json_fields "$response" "quote pcr_values timestamp signature" "Attestation Service" "POST /api/v1/quote" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    # Verify quote contract
    quote=$(echo "$response" | jq -r '.quote')
    response=$(curl -sf --max-time 5 -X POST "$ATTESTATION_SERVICE/api/v1/verify" \
        -H "Content-Type: application/json" \
        -d "{\"quote\":\"$quote\"}")
    validate_json_fields "$response" "valid pcr_values timestamp" "Attestation Service" "POST /api/v1/verify" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    # Metrics contract
    response=$(curl -sf --max-time 5 "$ATTESTATION_SERVICE/metrics")
    if echo "$response" | grep -q "sentinel_attestation_"; then
        ((TESTS_PASSED++))
    else
        log_error "Attestation Service /metrics: missing sentinel_attestation_ prefix"
        ((TESTS_FAILED++))
    fi
}

# Test Health Check contracts
test_health_check_contracts() {
    log_info "Testing Health Check API contracts..."

    # Health contract
    response=$(curl -sf --max-time 5 "$HEALTH_CHECK/health")
    validate_json_fields "$response" "status timestamp version" "Health Check" "/health" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    # Comprehensive health contract
    response=$(curl -sf --max-time 10 "$HEALTH_CHECK/api/v1/health")
    validate_json_fields "$response" "overall_status checks timestamp" "Health Check" "GET /api/v1/health" || ((TESTS_FAILED++))
    if echo "$response" | jq -e '.checks | type == "array"' >/dev/null; then
        ((TESTS_PASSED++))
    else
        log_error "Health Check GET /api/v1/health: checks not array"
        ((TESTS_FAILED++))
    fi

    # Individual checks contract
    response=$(curl -sf --max-time 5 "$HEALTH_CHECK/api/v1/checks")
    if echo "$response" | jq -e 'type == "array"' >/dev/null; then
        # Verify each check has required fields
        if echo "$response" | jq -e 'all(.[] | has("name") and has("status") and has("message") and has("timestamp"))' >/dev/null; then
            ((TESTS_PASSED++))
        else
            log_error "Health Check GET /api/v1/checks: missing required fields in check items"
            ((TESTS_FAILED++))
        fi
    else
        log_error "Health Check GET /api/v1/checks: response not array"
        ((TESTS_FAILED++))
    fi

    # Metrics contract
    response=$(curl -sf --max-time 5 "$HEALTH_CHECK/metrics")
    if echo "$response" | grep -q "sentinel_health_"; then
        ((TESTS_PASSED++))
    else
        log_error "Health Check /metrics: missing sentinel_health_ prefix"
        ((TESTS_FAILED++))
    fi
}

# Test Operator TUI contracts
test_operator_tui_contracts() {
    log_info "Testing Operator TUI API contracts..."

    # Health contract
    response=$(curl -sf --max-time 5 "$OPERATOR_TUI/health")
    validate_json_fields "$response" "status timestamp version" "Operator TUI" "/health" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    # Status contract
    response=$(curl -sf --max-time 5 "$OPERATOR_TUI/api/v1/status")
    validate_json_fields "$response" "version uptime services compartments tasks" "Operator TUI" "GET /api/v1/status" || ((TESTS_FAILED++))
    ((TESTS_PASSED++))

    # Metrics contract
    response=$(curl -sf --max-time 5 "$OPERATOR_TUI/metrics")
    if echo "$response" | grep -q "sentinel_operator_"; then
        ((TESTS_PASSED++))
    else
        log_error "Operator TUI /metrics: missing sentinel_operator_ prefix"
        ((TESTS_FAILED++))
    fi
}

# Main test runner
main() {
    log_info "Starting Sentinel OS Contract Tests"
    log_info "=================================="

    # Wait for services to be ready
    log_info "Waiting for services to be ready..."
    sleep 5

    test_capability_broker_contracts
    test_compartment_manager_contracts
    test_ai_orchestrator_contracts
    test_model_registry_contracts
    test_threat_intel_contracts
    test_attestation_service_contracts
    test_health_check_contracts
    test_operator_tui_contracts

    echo ""
    log_info "=================================="
    log_info "Contract Test Summary:"
    log_info "  Passed: $TESTS_PASSED"
    log_info "  Failed: $TESTS_FAILED"
    log_info "  Total:  $((TESTS_PASSED + TESTS_FAILED))"

    if [ $TESTS_FAILED -eq 0 ]; then
        log_info "All contract tests PASSED!"
        exit 0
    else
        log_error "Some contract tests FAILED!"
        exit 1
    fi
}

main "$@"