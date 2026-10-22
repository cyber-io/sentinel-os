-- Sentinel OS Database Initialization
-- Run on PostgreSQL startup

-- Create schemas
CREATE SCHEMA IF NOT EXISTS sentinel;
CREATE SCHEMA IF NOT EXISTS audit;

-- Create extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- =========================================================================
-- Capability Broker Tables
-- =========================================================================

CREATE TABLE IF NOT EXISTS sentinel.capability_policies (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    compartment_type VARCHAR(100) NOT NULL UNIQUE,
    policy_json JSONB NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS sentinel.capability_tokens (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    token_hash VARCHAR(64) NOT NULL UNIQUE,
    compartment_type VARCHAR(100) NOT NULL,
    capabilities JSONB NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ
);

CREATE INDEX idx_tokens_hash ON sentinel.capability_tokens(token_hash);
CREATE INDEX idx_tokens_expires ON sentinel.capability_tokens(expires_at);
CREATE INDEX idx_tokens_type ON sentinel.capability_tokens(compartment_type);

-- =========================================================================
-- Compartment Manager Tables
-- =========================================================================

CREATE TABLE IF NOT EXISTS sentinel.compartments (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL UNIQUE,
    compartment_type VARCHAR(100) NOT NULL,
    backend VARCHAR(50) NOT NULL,
    state VARCHAR(50) NOT NULL DEFAULT 'creating',
    pid INTEGER,
    ip_address INET,
    resources JSONB NOT NULL DEFAULT '{}',
    capabilities JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    stopped_at TIMESTAMPTZ
);

CREATE INDEX idx_compartments_state ON sentinel.compartments(state);
CREATE INDEX idx_compartments_type ON sentinel.compartments(compartment_type);

-- =========================================================================
-- AI Orchestrator Tables
-- =========================================================================

CREATE TABLE IF NOT EXISTS sentinel.tasks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    description TEXT NOT NULL,
    context JSONB NOT NULL DEFAULT '{}',
    priority INTEGER NOT NULL DEFAULT 1,
    required_capabilities JSONB NOT NULL DEFAULT '[]',
    target_compartment VARCHAR(255),
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    plan JSONB,
    current_step INTEGER NOT NULL DEFAULT 0,
    result JSONB,
    error TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ
);

CREATE INDEX idx_tasks_status ON sentinel.tasks(status);
CREATE INDEX idx_tasks_priority ON sentinel.tasks(priority DESC, created_at);
CREATE INDEX idx_tasks_compartment ON sentinel.tasks(target_compartment);

-- =========================================================================
-- Model Registry Tables
-- =========================================================================

CREATE TABLE IF NOT EXISTS sentinel.model_manifests (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL,
    tag VARCHAR(100) NOT NULL,
    digest VARCHAR(128) NOT NULL,
    size_bytes BIGINT NOT NULL,
    architecture VARCHAR(50) NOT NULL,
    os VARCHAR(50) NOT NULL,
    labels JSONB NOT NULL DEFAULT '{}',
    annotations JSONB NOT NULL DEFAULT '{}',
    layers JSONB NOT NULL DEFAULT '[]',
    config JSONB NOT NULL DEFAULT '{}',
    signatures JSONB NOT NULL DEFAULT '[]',
    sbom JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(name, tag)
);

CREATE INDEX idx_models_name ON sentinel.model_manifests(name);
CREATE INDEX idx_models_digest ON sentinel.model_manifests(digest);

-- =========================================================================
-- Threat Intelligence Tables
-- =========================================================================

CREATE TABLE IF NOT EXISTS sentinel.threat_indicators (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    indicator_type VARCHAR(50) NOT NULL,
    value VARCHAR(512) NOT NULL,
    threat_types JSONB NOT NULL DEFAULT '[]',
    confidence SMALLINT NOT NULL DEFAULT 50,
    severity VARCHAR(20) NOT NULL DEFAULT 'medium',
    description TEXT,
    first_seen TIMESTAMPTZ NOT NULL,
    last_seen TIMESTAMPTZ NOT NULL,
    tags JSONB NOT NULL DEFAULT '[]',
    mitre_techniques JSONB NOT NULL DEFAULT '[]',
    source VARCHAR(100) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(indicator_type, value, source)
);

CREATE INDEX idx_threats_type_value ON sentinel.threat_indicators(indicator_type, value);
CREATE INDEX idx_threats_severity ON sentinel.threat_indicators(severity);
CREATE INDEX idx_threats_last_seen ON sentinel.threat_indicators(last_seen);

-- =========================================================================
-- Attestation Tables
-- =========================================================================

CREATE TABLE IF NOT EXISTS sentinel.pcr_policies (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    pcr INTEGER NOT NULL UNIQUE,
    expected_digest VARCHAR(128),
    allow_list JSONB NOT NULL DEFAULT '[]',
    deny_list JSONB NOT NULL DEFAULT '[]',
    action VARCHAR(20) NOT NULL DEFAULT 'alert',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS sentinel.pcr_events (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    pcr INTEGER NOT NULL,
    old_digest VARCHAR(128) NOT NULL,
    new_digest VARCHAR(128) NOT NULL,
    event_data TEXT,
    action_taken VARCHAR(20) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_pcr_events_pcr ON sentinel.pcr_events(pcr);
CREATE INDEX idx_pcr_events_timestamp ON sentinel.pcr_events(timestamp);

-- =========================================================================
-- Health Check Tables
-- =========================================================================

CREATE TABLE IF NOT EXISTS sentinel.health_snapshots (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    overall_status VARCHAR(20) NOT NULL,
    checks JSONB NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_health_timestamp ON sentinel.health_snapshots(timestamp);

-- =========================================================================
-- Audit Log Table
-- =========================================================================

CREATE TABLE IF NOT EXISTS audit.events (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    event_type VARCHAR(100) NOT NULL,
    actor VARCHAR(255),
    resource_type VARCHAR(100),
    resource_id UUID,
    action VARCHAR(100) NOT NULL,
    result VARCHAR(50) NOT NULL,
    details JSONB,
    ip_address INET,
    user_agent TEXT,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_type ON audit.events(event_type);
CREATE INDEX idx_audit_actor ON audit.events(actor);
CREATE INDEX idx_audit_resource ON audit.events(resource_type, resource_id);
CREATE INDEX idx_audit_timestamp ON audit.events(timestamp);

-- =========================================================================
-- Default Data
-- =========================================================================

-- Insert default PCR policies
INSERT INTO sentinel.pcr_policies (pcr, expected_digest, allow_list, deny_list, action) VALUES
(0, NULL, '["/boot/vmlinuz-*", "/boot/initramfs-*"]', '[]', 'alert'),
(1, NULL, '["/boot/grub/*"]', '[]', 'alert'),
(2, NULL, '[]', '[]', 'alert'),
(3, NULL, '[]', '[]', 'alert'),
(4, NULL, '[]', '[]', 'alert'),
(5, NULL, '[]', '[]', 'alert'),
(6, NULL, '[]', '[]', 'alert'),
(7, NULL, '[]', '[]', 'alert')
ON CONFLICT (pcr) DO NOTHING;

-- Insert default capability policies
INSERT INTO sentinel.capability_policies (compartment_type, policy_json) VALUES
('network-scan', '{"allow": ["cap_net_raw", "cap_net_admin"], "deny": ["cap_sys_admin", "cap_sys_module"], "resources": {"cpu": 2, "memory": "4GB", "disk": "10GB"}}'),
('vuln-scan', '{"allow": ["cap_net_raw", "cap_net_admin", "cap_dac_read_search"], "deny": ["cap_sys_admin", "cap_sys_module", "cap_sys_ptrace"], "resources": {"cpu": 4, "memory": "8GB", "disk": "20GB"}}'),
('exploitation', '{"allow": ["cap_net_raw", "cap_net_admin", "cap_dac_override"], "deny": ["cap_sys_admin", "cap_sys_module", "cap_sys_ptrace", "cap_sys_resource"], "resources": {"cpu": 4, "memory": "8GB", "disk": "20GB"}}'),
('forensics', '{"allow": ["cap_dac_read_search", "cap_sys_admin"], "deny": ["cap_net_raw", "cap_net_admin", "cap_sys_module"], "resources": {"cpu": 4, "memory": "16GB", "disk": "50GB"}}'),
('analysis', '{"allow": ["cap_dac_read_search"], "deny": ["cap_net_raw", "cap_net_admin", "cap_sys_admin", "cap_sys_module"], "resources": {"cpu": 2, "memory": "4GB", "disk": "10GB"}}')
ON CONFLICT (compartment_type) DO NOTHING;

-- Grant permissions
GRANT USAGE ON SCHEMA sentinel TO sentinel;
GRANT USAGE ON SCHEMA audit TO sentinel;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA sentinel TO sentinel;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA audit TO sentinel;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA sentinel TO sentinel;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA audit TO sentinel;

-- Set default privileges
ALTER DEFAULT PRIVILEGES IN SCHEMA sentinel GRANT ALL ON TABLES TO sentinel;
ALTER DEFAULT PRIVILEGES IN SCHEMA sentinel GRANT ALL ON SEQUENCES TO sentinel;
ALTER DEFAULT PRIVILEGES IN SCHEMA audit GRANT ALL ON TABLES TO sentinel;
ALTER DEFAULT PRIVILEGES IN SCHEMA audit GRANT ALL ON SEQUENCES TO sentinel;