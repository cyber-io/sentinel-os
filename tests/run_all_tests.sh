#!/bin/bash
# Sentinel OS Test Runner
# Runs all integration and contract tests

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_step() { echo -e "${BLUE}[STEP]${NC} $1"; }

# Test suite results
INTEGRATION_RESULT=0
CONTRACT_RESULT=0

run_integration_tests() {
    log_step "Running Integration Tests..."
    if bash "$SCRIPT_DIR/integration/test_service_communication.sh"; then
        log_info "Integration tests: PASSED"
        INTEGRATION_RESULT=0
    else
        log_error "Integration tests: FAILED"
        INTEGRATION_RESULT=1
    fi
}

run_contract_tests() {
    log_step "Running Contract Tests..."
    if bash "$SCRIPT_DIR/contracts/test_api_contracts.sh"; then
        log_info "Contract tests: PASSED"
        CONTRACT_RESULT=0
    else
        log_error "Contract tests: FAILED"
        CONTRACT_RESULT=1
    fi
}

run_unit_tests() {
    log_step "Running Unit Tests (Rust)..."
    cd "$PROJECT_ROOT"

    # Run unit tests for each service
    for service in capability-broker compartment-manager ai-orchestrator model-registry threat-intel attestation-service health-check operator-tui; do
        log_info "Testing $service..."
        if cargo test --manifest-path "sentinel/src/$service/Cargo.toml" 2>&1 | tail -20; then
            log_info "$service unit tests: PASSED"
        else
            log_error "$service unit tests: FAILED"
            INTEGRATION_RESULT=1
        fi
    done
}

run_lint_checks() {
    log_step "Running Lint Checks..."

    cd "$PROJECT_ROOT"

    # Check formatting
    log_info "Checking Rust formatting..."
    if cargo fmt --all --check 2>&1; then
        log_info "Rust fmt: PASSED"
    else
        log_error "Rust fmt: FAILED"
        INTEGRATION_RESULT=1
    fi

    # Clippy
    log_info "Running Clippy..."
    if cargo clippy --all-targets --all-features -- -D warnings 2>&1; then
        log_info "Clippy: PASSED"
    else
        log_error "Clippy: FAILED"
        INTEGRATION_RESULT=1
    fi

    # Check for security issues
    log_info "Running cargo audit..."
    if command -v cargo-audit &> /dev/null; then
        if cargo audit 2>&1; then
            log_info "Cargo audit: PASSED"
        else
            log_warn "Cargo audit: Found vulnerabilities (check output)"
        fi
    else
        log_warn "cargo-audit not installed, skipping"
    fi
}

run_nixos_checks() {
    log_step "Running NixOS Configuration Checks..."

    cd "$PROJECT_ROOT"

    # Check flake syntax
    log_info "Checking Nix flake..."
    if nix flake check 2>&1; then
        log_info "Nix flake check: PASSED"
    else
        log_error "Nix flake check: FAILED"
        INTEGRATION_RESULT=1
    fi

    # Check each configuration builds
    for config in dev-vm hardened installer; do
        log_info "Building NixOS configuration: $config"
        if nix build ".#nixosConfigurations.$config.config.system.build.toplevel" --no-link 2>&1; then
            log_info "NixOS $config build: PASSED"
        else
            log_error "NixOS $config build: FAILED"
            INTEGRATION_RESULT=1
        fi
    done
}

print_summary() {
    echo ""
    log_step "Test Summary"
    echo "=============="
    echo -e "Integration Tests: $([ $INTEGRATION_RESULT -eq 0 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
    echo -e "Contract Tests:    $([ $CONTRACT_RESULT -eq 0 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"

    if [ $INTEGRATION_RESULT -eq 0 ] && [ $CONTRACT_RESULT -eq 0 ]; then
        log_info "All tests PASSED!"
        exit 0
    else
        log_error "Some tests FAILED!"
        exit 1
    fi
}

# Main
main() {
    log_info "Sentinel OS Test Suite"
    log_info "======================"

    case "${1:-all}" in
        integration)
            run_integration_tests
            ;;
        contract)
            run_contract_tests
            ;;
        unit)
            run_unit_tests
            ;;
        lint)
            run_lint_checks
            ;;
        nixos)
            run_nixos_checks
            ;;
        all)
            run_lint_checks
            run_unit_tests
            run_integration_tests
            run_contract_tests
            run_nixos_checks
            ;;
        *)
            log_error "Unknown test suite: $1"
            echo "Usage: $0 [integration|contract|unit|lint|nixos|all]"
            exit 1
            ;;
    esac

    print_summary
}

main "$@"