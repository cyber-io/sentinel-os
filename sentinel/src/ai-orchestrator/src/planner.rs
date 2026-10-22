//! Planner module - generates execution plans from natural language tasks

use std::sync::Arc;
use async_trait::async_trait;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::models::{ModelRegistry, InferenceRequest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub description: String,
    pub steps: Vec<PlanStep>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub step_type: String,
    pub description: String,
    pub parameters: Value,
    pub dependencies: Vec<usize>, // Indices of steps that must complete first
    pub status: StepStatus,
    pub result: Option<Value>,
    pub estimated_duration_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

pub struct Planner {
    model_registry: Arc<ModelRegistry>,
    model_name: String,
    system_prompt: String,
}

impl Planner {
    pub async fn new(model_registry: Arc<ModelRegistry>, model_name: String) -> Result<Self> {
        let system_prompt = include_str!("prompts/planner_system.txt").to_string();
        Ok(Self { model_registry, model_name, system_prompt })
    }

    pub async fn generate_plan(&self, task: &str, context: &Value) -> Result<Plan> {
        let prompt = self.build_prompt(task, context);

        let req = InferenceRequest {
            model: self.model_name.clone(),
            prompt,
            system_prompt: Some(self.system_prompt.clone()),
            temperature: 0.3,
            max_tokens: 4096,
            stream: false,
        };

        let resp = self.model_registry.infer(req).await?;
        self.parse_plan(&resp.text, task)
    }

    fn build_prompt(&self, task: &str, context: &Value) -> String {
        format!(r#"Task: {}

Context: {}

Generate a detailed execution plan as JSON with the following structure:
{{
  "steps": [
    {{
      "step_type": "inference|compartment_action|tool|wait",
      "description": "Human-readable description",
      "parameters": {{}},
      "dependencies": [],
      "estimated_duration_secs": 30
    }}
  ]
}}

Available step types:
- inference: Run LLM inference (parameters: model, prompt, system_prompt, temperature, max_tokens)
- compartment_action: Execute command in compartment (parameters: compartment, action, command[])
- tool: Run CLI tool (parameters: tool, args[])
- wait: Wait for duration (parameters: duration)

Focus on security operations: reconnaissance, vulnerability scanning, exploitation, post-exploitation, reporting."#, task, serde_json::to_string_pretty(context).unwrap_or_default())
    }

    fn parse_plan(&self, response: &str, task: &str) -> Result<Plan> {
        // Try to extract JSON from response
        let json_str = self.extract_json(response)?;
        let plan_data: serde_json::Value = serde_json::from_str(&json_str)?;

        let mut steps = Vec::new();
        if let Some(steps_array) = plan_data.get("steps").and_then(|v| v.as_array()) {
            for (idx, step_val) in steps_array.iter().enumerate() {
                let step = PlanStep {
                    id: Uuid::new_v4().to_string(),
                    step_type: step_val.get("step_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("inference")
                        .to_string(),
                    description: step_val.get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&format!("Step {}", idx + 1))
                        .to_string(),
                    parameters: step_val.get("parameters").cloned().unwrap_or(Value::Object(Default::default())),
                    dependencies: step_val.get("dependencies")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|u| u as usize)).collect())
                        .unwrap_or_default(),
                    status: StepStatus::Pending,
                    result: None,
                    estimated_duration_secs: step_val.get("estimated_duration_secs")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(60),
                };
                steps.push(step);
            }
        }

        Ok(Plan {
            id: Uuid::new_v4().to_string(),
            description: task.to_string(),
            steps,
            metadata: Value::Object(Default::default()),
        })
    }

    fn extract_json(&self, text: &str) -> Result<String> {
        // Find JSON block in response
        if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                return Ok(text[start..=end].to_string());
            }
        }
        // Fallback: assume entire response is JSON
        Ok(text.to_string())
    }
}

use uuid::Uuid;