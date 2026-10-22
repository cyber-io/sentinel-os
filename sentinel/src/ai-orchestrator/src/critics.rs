//! Critics module - validates plans for safety, efficacy, and correctness

use std::sync::Arc;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::models::{ModelRegistry, InferenceRequest};
use crate::planner::{Plan, PlanStep};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriticResult {
    pub critic_type: CriticType,
    pub passed: bool,
    pub score: f32,
    pub findings: Vec<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CriticType {
    Safety,
    Efficacy,
    Legality,
    OperationalSecurity,
    ResourceEfficiency,
}

pub struct CriticManager {
    model_registry: Arc<ModelRegistry>,
    critics: Vec<Box<dyn Critic>>,
}

#[async_trait::async_trait]
trait Critic: Send + Sync {
    fn critic_type(&self) -> CriticType;
    async fn evaluate(&self, plan: &Plan, context: &Value) -> Result<CriticResult>;
}

struct SafetyCritic {
    model_registry: Arc<ModelRegistry>,
    model_name: String,
}

struct EfficacyCritic {
    model_registry: Arc<ModelRegistry>,
    model_name: String,
}

struct LegalityCritic {
    model_registry: Arc<ModelRegistry>,
    model_name: String,
}

struct OpSecCritic {
    model_registry: Arc<ModelRegistry>,
    model_name: String,
}

struct ResourceCritic {
    model_registry: Arc<ModelRegistry>,
    model_name: String,
}

impl CriticManager {
    pub async fn new(model_registry: Arc<ModelRegistry>, critic_models: Vec<String>) -> Result<Self> {
        let mut critics: Vec<Box<dyn Critic>> = Vec::new();

        // Default critic models if not specified
        let models = if critic_models.is_empty() {
            vec![
                "sentinel-critic-safety".to_string(),
                "sentinel-critic-efficacy".to_string(),
                "sentinel-critic-legality".to_string(),
                "sentinel-critic-opsec".to_string(),
                "sentinel-critic-resource".to_string(),
            ]
        } else {
            critic_models
        };

        // Create critics with their respective models
        for (i, model) in models.iter().enumerate() {
            let critic: Box<dyn Critic> = match i {
                0 => Box::new(SafetyCritic::new(model_registry.clone(), model.clone())),
                1 => Box::new(EfficacyCritic::new(model_registry.clone(), model.clone())),
                2 => Box::new(LegalityCritic::new(model_registry.clone(), model.clone())),
                3 => Box::new(OpSecCritic::new(model_registry.clone(), model.clone())),
                4 => Box::new(ResourceCritic::new(model_registry.clone(), model.clone())),
                _ => Box::new(SafetyCritic::new(model_registry.clone(), model.clone())),
            };
            critics.push(critic);
        }

        Ok(Self { model_registry, critics })
    }

    pub async fn validate_plan(&self, plan: &Plan, context: &Value) -> Result<CriticValidationResult> {
        let mut results = Vec::new();
        let mut total_score = 0.0;

        for critic in &self.critics {
            let result = critic.evaluate(plan, context).await?;
            total_score += result.score;
            results.push(result);
        }

        let overall_score = total_score / self.critics.len().max(1) as f32;
        let passed = results.iter().all(|r| r.passed) && overall_score >= 0.7;

        Ok(CriticValidationResult {
            passed,
            results,
            overall_score,
        })
    }
}

#[derive(Debug)]
pub struct CriticValidationResult {
    pub passed: bool,
    pub results: Vec<CriticResult>,
    pub overall_score: f32,
}

impl SafetyCritic {
    fn new(model_registry: Arc<ModelRegistry>, model_name: String) -> Self {
        Self { model_registry, model_name }
    }
}

#[async_trait::async_trait]
impl Critic for SafetyCritic {
    fn critic_type(&self) -> CriticType {
        CriticType::Safety
    }

    async fn evaluate(&self, plan: &Plan, context: &Value) -> Result<CriticResult> {
        let prompt = self.build_prompt(plan, context);

        let req = InferenceRequest {
            model: self.model_name.clone(),
            prompt,
            system_prompt: Some(self.system_prompt()),
            temperature: 0.1,
            max_tokens: 2048,
            stream: false,
        };

        let resp = self.model_registry.infer(req).await?;
        self.parse_result(&resp.text)
    }

    fn system_prompt(&self) -> String {
        r#"You are a Safety Critic for an AI-powered security operating system.
Evaluate plans for potential safety risks including:
1. Unintended damage to systems or data
2. Escalation beyond authorized scope
3. Persistence mechanisms that could be abused
4. Lateral movement capabilities
5. Data exfiltration potential
6. Denial of service risks

Score from 0.0 (unsafe) to 1.0 (safe).
Return JSON: {"passed": bool, "score": float, "findings": [], "recommendations": []}"#.to_string()
    }

    fn build_prompt(&self, plan: &Plan, context: &Value) -> String {
        format!(r#"Evaluate this security operation plan for safety risks:

Plan: {}
Steps:
{}

Context: {}

Assess each step for safety concerns. Consider:
- Could this cause unintended system damage?
- Does it exceed authorized scope?
- Could it create persistent access?
- Does it enable lateral movement?
- Is there data exfiltration risk?
- Could it cause denial of service?

Return JSON only."#,
            plan.description,
            plan.steps.iter().enumerate().map(|(i, s)| format!("{}. {} ({})", i+1, s.description, s.step_type)).collect::<Vec<_>>().join("\n"),
            serde_json::to_string_pretty(context).unwrap_or_default())
    }

    fn parse_result(&self, text: &str) -> Result<CriticResult> {
        let json_str = self.extract_json(text)?;
        let val: Value = serde_json::from_str(&json_str)?;

        Ok(CriticResult {
            critic_type: CriticType::Safety,
            passed: val.get("passed").and_then(|v| v.as_bool()).unwrap_or(false),
            score: val.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            findings: val.get("findings").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
            recommendations: val.get("recommendations").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
        })
    }

    fn extract_json(&self, text: &str) -> Result<String> {
        if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                return Ok(text[start..=end].to_string());
            }
        }
        Ok(text.to_string())
    }
}

impl EfficacyCritic {
    fn new(model_registry: Arc<ModelRegistry>, model_name: String) -> Self {
        Self { model_registry, model_name }
    }
}

#[async_trait::async_trait]
impl Critic for EfficacyCritic {
    fn critic_type(&self) -> CriticType {
        CriticType::Efficacy
    }

    async fn evaluate(&self, plan: &Plan, context: &Value) -> Result<CriticResult> {
        let prompt = self.build_prompt(plan, context);

        let req = InferenceRequest {
            model: self.model_name.clone(),
            prompt,
            system_prompt: Some(self.system_prompt()),
            temperature: 0.1,
            max_tokens: 2048,
            stream: false,
        };

        let resp = self.model_registry.infer(req).await?;
        self.parse_result(&resp.text)
    }

    fn system_prompt(&self) -> String {
        r#"You are an Efficacy Critic for security operations.
Evaluate plans for effectiveness:
1. Will the plan achieve the stated objective?
2. Are the steps logically ordered?
3. Are appropriate tools/techniques selected?
4. Is there sufficient coverage?
5. Are success criteria measurable?
6. Are there gaps or missing steps?

Score from 0.0 (ineffective) to 1.0 (highly effective).
Return JSON: {"passed": bool, "score": float, "findings": [], "recommendations": []}"#.to_string()
    }

    fn build_prompt(&self, plan: &Plan, context: &Value) -> String {
        format!(r#"Evaluate this security operation plan for efficacy:

Plan: {}
Steps:
{}

Context: {}

Assess:
- Will this achieve the objective?
- Logical step ordering?
- Appropriate tool selection?
- Coverage completeness?
- Measurable success criteria?
- Missing steps or gaps?

Return JSON only."#,
            plan.description,
            plan.steps.iter().enumerate().map(|(i, s)| format!("{}. {} ({})", i+1, s.description, s.step_type)).collect::<Vec<_>>().join("\n"),
            serde_json::to_string_pretty(context).unwrap_or_default())
    }

    fn parse_result(&self, text: &str) -> Result<CriticResult> {
        let json_str = self.extract_json(text)?;
        let val: Value = serde_json::from_str(&json_str)?;

        Ok(CriticResult {
            critic_type: CriticType::Efficacy,
            passed: val.get("passed").and_then(|v| v.as_bool()).unwrap_or(false),
            score: val.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            findings: val.get("findings").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
            recommendations: val.get("recommendations").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
        })
    }

    fn extract_json(&self, text: &str) -> Result<String> {
        if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                return Ok(text[start..=end].to_string());
            }
        }
        Ok(text.to_string())
    }
}

impl LegalityCritic {
    fn new(model_registry: Arc<ModelRegistry>, model_name: String) -> Self {
        Self { model_registry, model_name }
    }
}

#[async_trait::async_trait]
impl Critic for LegalityCritic {
    fn critic_type(&self) -> CriticType {
        CriticType::Legality
    }

    async fn evaluate(&self, plan: &Plan, context: &Value) -> Result<CriticResult> {
        let prompt = self.build_prompt(plan, context);

        let req = InferenceRequest {
            model: self.model_name.clone(),
            prompt,
            system_prompt: Some(self.system_prompt()),
            temperature: 0.0,
            max_tokens: 2048,
            stream: false,
        };

        let resp = self.model_registry.infer(req).await?;
        self.parse_result(&resp.text)
    }

    fn system_prompt(&self) -> String {
        r#"You are a Legality Critic for security operations.
Evaluate plans for legal compliance:
1. Authorization and scope compliance
2. Applicable laws (CFAA, GDPR, local cyber laws)
3. Rules of engagement adherence
4. Data protection requirements
5. Reporting obligations
6. Chain of custody for evidence

Score from 0.0 (illegal) to 1.0 (fully compliant).
Return JSON: {"passed": bool, "score": float, "findings": [], "recommendations": []}"#.to_string()
    }

    fn build_prompt(&self, plan: &Plan, context: &Value) -> String {
        format!(r#"Evaluate this security operation plan for legal compliance:

Plan: {}
Steps:
{}

Context: {}

Assess:
- Proper authorization documented?
- Within defined scope?
- Compliance with applicable laws?
- Rules of engagement followed?
- Data protection considered?
- Evidence handling procedures?

Return JSON only."#,
            plan.description,
            plan.steps.iter().enumerate().map(|(i, s)| format!("{}. {} ({})", i+1, s.description, s.step_type)).collect::<Vec<_>>().join("\n"),
            serde_json::to_string_pretty(context).unwrap_or_default())
    }

    fn parse_result(&self, text: &str) -> Result<CriticResult> {
        let json_str = self.extract_json(text)?;
        let val: Value = serde_json::from_str(&json_str)?;

        Ok(CriticResult {
            critic_type: CriticType::Legality,
            passed: val.get("passed").and_then(|v| v.as_bool()).unwrap_or(false),
            score: val.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            findings: val.get("findings").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
            recommendations: val.get("recommendations").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
        })
    }

    fn extract_json(&self, text: &str) -> Result<String> {
        if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                return Ok(text[start..=end].to_string());
            }
        }
        Ok(text.to_string())
    }
}

impl OpSecCritic {
    fn new(model_registry: Arc<ModelRegistry>, model_name: String) -> Self {
        Self { model_registry, model_name }
    }
}

#[async_trait::async_trait]
impl Critic for OpSecCritic {
    fn critic_type(&self) -> CriticType {
        CriticType::OperationalSecurity
    }

    async fn evaluate(&self, plan: &Plan, context: &Value) -> Result<CriticResult> {
        let prompt = self.build_prompt(plan, context);

        let req = InferenceRequest {
            model: self.model_name.clone(),
            prompt,
            system_prompt: Some(self.system_prompt()),
            temperature: 0.1,
            max_tokens: 2048,
            stream: false,
        };

        let resp = self.model_registry.infer(req).await?;
        self.parse_result(&resp.text)
    }

    fn system_prompt(&self) -> String {
        r#"You are an Operational Security Critic.
Evaluate plans for OPSEC:
1. Attribution risk (tools, techniques, infrastructure)
2. Network signature and detection avoidance
3. Artifact cleanup and anti-forensics
4. Communication security (encryption, rotation)
5. Identity and access management
6. Persistence vs. transience trade-offs

Score from 0.0 (high exposure) to 1.0 (excellent OPSEC).
Return JSON: {"passed": bool, "score": float, "findings": [], "recommendations": []}"#.to_string()
    }

    fn build_prompt(&self, plan: &Plan, context: &Value) -> String {
        format!(r#"Evaluate this security operation plan for operational security:

Plan: {}
Steps:
{}

Context: {}

Assess:
- Attribution risk from tools/techniques?
- Network signature minimization?
- Artifact cleanup planned?
- Encrypted communications?
- Identity protection?
- Appropriate persistence level?

Return JSON only."#,
            plan.description,
            plan.steps.iter().enumerate().map(|(i, s)| format!("{}. {} ({})", i+1, s.description, s.step_type)).collect::<Vec<_>>().join("\n"),
            serde_json::to_string_pretty(context).unwrap_or_default())
    }

    fn parse_result(&self, text: &str) -> Result<CriticResult> {
        let json_str = self.extract_json(text)?;
        let val: Value = serde_json::from_str(&json_str)?;

        Ok(CriticResult {
            critic_type: CriticType::OperationalSecurity,
            passed: val.get("passed").and_then(|v| v.as_bool()).unwrap_or(false),
            score: val.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            findings: val.get("findings").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
            recommendations: val.get("recommendations").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
        })
    }

    fn extract_json(&self, text: &str) -> Result<String> {
        if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                return Ok(text[start..=end].to_string());
            }
        }
        Ok(text.to_string())
    }
}

impl ResourceCritic {
    fn new(model_registry: Arc<ModelRegistry>, model_name: String) -> Self {
        Self { model_registry, model_name }
    }
}

#[async_trait::async_trait]
impl Critic for ResourceCritic {
    fn critic_type(&self) -> CriticType {
        CriticType::ResourceEfficiency
    }

    async fn evaluate(&self, plan: &Plan, context: &Value) -> Result<CriticResult> {
        let prompt = self.build_prompt(plan, context);

        let req = InferenceRequest {
            model: self.model_name.clone(),
            prompt,
            system_prompt: Some(self.system_prompt()),
            temperature: 0.1,
            max_tokens: 2048,
            stream: false,
        };

        let resp = self.model_registry.infer(req).await?;
        self.parse_result(&resp.text)
    }

    fn system_prompt(&self) -> String {
        r#"You are a Resource Efficiency Critic.
Evaluate plans for resource usage:
1. Computational efficiency (CPU, GPU, memory)
2. Network bandwidth usage
3. Storage requirements
4. Time efficiency
5. Cost optimization (cloud/API costs)
6. Parallelization opportunities

Score from 0.0 (wasteful) to 1.0 (highly efficient).
Return JSON: {"passed": bool, "score": float, "findings": [], "recommendations": []}"#.to_string()
    }

    fn build_prompt(&self, plan: &Plan, context: &Value) -> String {
        format!(r#"Evaluate this security operation plan for resource efficiency:

Plan: {}
Steps:
{}

Context: {}

Assess:
- Computational efficiency?
- Network bandwidth usage?
- Storage requirements?
- Time efficiency?
- Cost optimization?
- Parallelization opportunities?

Return JSON only."#,
            plan.description,
            plan.steps.iter().enumerate().map(|(i, s)| format!("{}. {} ({})", i+1, s.description, s.step_type)).collect::<Vec<_>>().join("\n"),
            serde_json::to_string_pretty(context).unwrap_or_default())
    }

    fn parse_result(&self, text: &str) -> Result<CriticResult> {
        let json_str = self.extract_json(text)?;
        let val: Value = serde_json::from_str(&json_str)?;

        Ok(CriticResult {
            critic_type: CriticType::ResourceEfficiency,
            passed: val.get("passed").and_then(|v| v.as_bool()).unwrap_or(false),
            score: val.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            findings: val.get("findings").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
            recommendations: val.get("recommendations").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
        })
    }

    fn extract_json(&self, text: &str) -> Result<String> {
        if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                return Ok(text[start..=end].to_string());
            }
        }
        Ok(text.to_string())
    }
}