//! Task queue module - manages task scheduling and prioritization

use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use async_trait::async_trait;
use anyhow::Result;
use dashmap::DashMap;
use parking_lot::RwLock;
use tokio::sync::{mpsc, Mutex, Notify};
use uuid::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum TaskPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Planning,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub context: serde_json::Value,
    pub priority: TaskPriority,
    pub required_capabilities: Vec<String>,
    pub target_compartment: Option<String>,
    pub status: TaskStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub retry_count: u32,
}

#[derive(Debug, Eq)]
struct QueuedTask {
    task: Task,
    enqueued_at: Instant,
}

impl PartialEq for QueuedTask {
    fn eq(&self, other: &Self) -> bool {
        self.task.priority == other.task.priority && self.enqueued_at == other.enqueued_at
    }
}

impl PartialOrd for QueuedTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueuedTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher priority first, then earlier enqueued first
        other.task.priority.cmp(&self.task.priority)
            .then_with(|| self.enqueued_at.cmp(&other.enqueued_at))
    }
}

pub struct TaskQueue {
    queue: Arc<Mutex<BinaryHeap<QueuedTask>>>,
    processing: Arc<DashMap<String, Task>>,
    completed: Arc<RwLock<HashMap<String, Task>>>,
    max_concurrent: usize,
    notify: Arc<Notify>,
    sender: mpsc::UnboundedSender<Task>,
    receiver: Arc<Mutex<mpsc::UnboundedReceiver<Task>>>,
}

impl TaskQueue {
    pub fn new(max_concurrent: usize) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();

        Self {
            queue: Arc::new(Mutex::new(BinaryHeap::new())),
            processing: Arc::new(DashMap::new()),
            completed: Arc::new(RwLock::new(HashMap::new())),
            max_concurrent,
            notify: Arc::new(Notify::new()),
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }

    pub async fn enqueue(&self, task: Task) -> Result<()> {
        let queued = QueuedTask {
            task: task.clone(),
            enqueued_at: Instant::now(),
        };

        self.queue.lock().await.push(queued);
        self.notify.notify_one();
        Ok(())
    }

    pub async fn dequeue(&self) -> Option<Task> {
        let mut queue = self.queue.lock().await;
        queue.pop().map(|qt| qt.task)
    }

    pub async fn try_dequeue(&self) -> Option<Task> {
        if self.processing.len() >= self.max_concurrent {
            return None;
        }

        let mut queue = self.queue.lock().await;
        queue.pop().map(|qt| {
            let task = qt.task;
            self.processing.insert(task.id.clone(), task.clone());
            task
        })
    }

    pub async fn mark_processing(&self, task: Task) {
        self.processing.insert(task.id.clone(), task);
    }

    pub async fn mark_completed(&self, mut task: Task) {
        task.status = TaskStatus::Completed;
        task.updated_at = chrono::Utc::now();
        self.processing.remove(&task.id);
        self.completed.write().insert(task.id.clone(), task);
    }

    pub async fn mark_failed(&self, mut task: Task, error: String) {
        task.status = TaskStatus::Failed;
        task.updated_at = chrono::Utc::now();
        self.processing.remove(&task.id);
        self.completed.write().insert(task.id.clone(), task);
    }

    pub async fn get_status(&self, task_id: &str) -> Option<TaskStatus> {
        if let Some(task) = self.processing.get(task_id) {
            return Some(task.status.clone());
        }
        self.completed.read().get(task_id).map(|t| t.status.clone())
    }

    pub async fn get_task(&self, task_id: &str) -> Option<Task> {
        if let Some(task) = self.processing.get(task_id) {
            return Some(task.clone());
        }
        self.completed.read().get(task_id).cloned()
    }

    pub async fn len(&self) -> usize {
        let queue_len = self.queue.lock().await.len();
        queue_len + self.processing.len()
    }

    pub async fn pending_count(&self) -> usize {
        self.queue.lock().await.len()
    }

    pub async fn processing_count(&self) -> usize {
        self.processing.len()
    }

    pub async fn completed_count(&self) -> usize {
        self.completed.read().len()
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<Task> {
        self.sender.clone()
    }

    pub async fn wait_for_task(&self) -> Option<Task> {
        loop {
            if let Some(task) = self.try_dequeue().await {
                return Some(task);
            }
            self.notify.notified().await;
        }
    }

    pub async fn cancel(&self, task_id: &str) -> bool {
        // Try to remove from queue (difficult with BinaryHeap, mark as cancelled instead)
        if let Some(mut task) = self.processing.remove(task_id).map(|(_, t)| t) {
            task.status = TaskStatus::Cancelled;
            task.updated_at = chrono::Utc::now();
            self.completed.write().insert(task_id.to_string(), task);
            return true;
        }
        false
    }

    pub async fn requeue(&self, mut task: Task) -> Result<()> {
        task.retry_count += 1;
        task.status = TaskStatus::Pending;
        task.updated_at = chrono::Utc::now();
        self.enqueue(task).await
    }
}

// Background worker that processes the queue
pub struct QueueWorker {
    queue: Arc<TaskQueue>,
    handler: Arc<dyn TaskHandler>,
}

#[async_trait::async_trait]
pub trait TaskHandler: Send + Sync {
    async fn handle(&self, task: Task) -> Result<Task>;
}

impl QueueWorker {
    pub fn new(queue: Arc<TaskQueue>, handler: Arc<dyn TaskHandler>) -> Self {
        Self { queue, handler }
    }

    pub async fn run(&self) {
        loop {
            if let Some(task) = self.queue.wait_for_task().await {
                let handler = self.handler.clone();
                let queue = self.queue.clone();

                tokio::spawn(async move {
                    let result = handler.handle(task.clone()).await;

                    match result {
                        Ok(completed_task) => {
                            queue.mark_completed(completed_task).await;
                        }
                        Err(e) => {
                            let mut failed_task = task;
                            failed_task.status = TaskStatus::Failed;
                            queue.mark_failed(failed_task, e.to_string()).await;
                        }
                    }
                });
            }
        }
    }
}