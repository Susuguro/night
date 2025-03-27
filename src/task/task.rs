use crate::common::types::{TaskConfig, TaskDependencyMap, TaskInfo, TaskStatus};
use crate::event::{Event, EventSystem, EventType};
use crate::utils::error::{NightError, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::time::{interval, Duration};
use uuid::Uuid;


static GLOBAL_EXECUTION_ORDER: AtomicUsize = AtomicUsize::new(0);
const MAX_RETRY_ATTEMPTS: u32 = 5;
const RETRY_DELAY: Duration = Duration::from_secs(1);

pub struct Task {
    pub config: TaskConfig,
    pub status: Arc<Mutex<TaskStatus>>,
    pub start_time: Arc<Mutex<Option<DateTime<Utc>>>>,
    pub end_time: Arc<Mutex<Option<DateTime<Utc>>>>,
    pub execution_lock: Arc<AtomicBool>,
    pub dependency_status: Arc<Mutex<TaskDependencyMap>>,
    pub execution_order: Arc<Mutex<Option<usize>>>,
    pub dependent_tasks: Arc<Vec<Uuid>>,
    pub event_system: Arc<EventSystem>,
}

impl Task {
    pub fn new(
        config: TaskConfig,
        event_system: Arc<EventSystem>,
        dependent_tasks: Vec<Uuid>,
    ) -> Self {
        let dependency_status: Arc<Mutex<HashMap<Uuid, bool>>> = Arc::new(Mutex::new(
            config.dependencies.iter().map(|&id| (id, false)).collect(),
        ));

        Task {
            config,
            status: Arc::new(Mutex::new(TaskStatus::Pending)),
            start_time: Arc::new(Mutex::new(None)),
            end_time: Arc::new(Mutex::new(None)),
            execution_lock: Arc::new(AtomicBool::new(true)),
            dependency_status,
            execution_order: Arc::new(Mutex::new(None)),
            dependent_tasks: Arc::new(dependent_tasks),
            event_system,
        }
    }

    pub async fn run(&self) -> Result<()> {
        // if !self.can_start().await {
        //     return Ok(());
        // }
        for attempt in 1..=MAX_RETRY_ATTEMPTS {
            if self.can_start().await {
                break;
            }
            if attempt == MAX_RETRY_ATTEMPTS {
                return Err(NightError::Task(format!(
                    "Task {} failed to start after {} attempts",
                    self.config.name, MAX_RETRY_ATTEMPTS
                )));
            }
            println!(
                "Task {}: Waiting for dependencies, attempt {}/{}",
                self.config.name, attempt, MAX_RETRY_ATTEMPTS
            );
            tokio::time::sleep(RETRY_DELAY).await;
        }

        // 检查执行锁，如果为false则不执行任务
        if !self.execution_lock.load(Ordering::Relaxed) {
            println!("Task {}: Execution locked, not running", self.config.name);
            return Ok(());
        }

        let order = GLOBAL_EXECUTION_ORDER.fetch_add(1, Ordering::SeqCst);

        if let Ok(mut guard) = self.execution_order.lock() {
            *guard = Some(order);
        } else {
            // Handle the case where the mutex is poisoned
            println!(
                "Warning: Failed to set execution order for task {}",
                self.config.name
            );
        }

        // 记录任务开始时间
        if let Ok(mut start_time) = self.start_time.lock() {
            *start_time = Some(chrono::Utc::now());
        }

        // println!("Task: Starting execution of {}", self.config.name);
        self.set_status(TaskStatus::Running).await;

        // let result = self.run_once().await;
        let result = if self.config.is_periodic {
            self.run_periodic().await
        } else {
            self.run_once().await
        };

        // 记录任务结束时间
        if let Ok(mut end_time) = self.end_time.lock() {
            *end_time = Some(chrono::Utc::now());
        }

        match &result {
            Ok(_) => {
                // println!("Task: Successfully completed {}", self.config.name);
                if !self.config.is_periodic {
                    self.set_status(TaskStatus::Completed).await;
                    println!("Task: Successfully completed {}", self.config.name);
                    self.notify_completion().await?;
                }
            }
            Err(e) => {
                println!("Task: Failed to execute {}. Error: {:?}", self.config.name, e);
                self.set_status(TaskStatus::Failed).await;
                // 失败任务也需要通知依赖它的任务
                self.notify_completion().await?;
            }
        }

        result
    }

    pub async fn get_execution_order(&self) -> Option<usize> {
        // Safely get the execution order
        self.execution_order.lock().ok().and_then(|guard| *guard)
    }

    pub async fn run_once(&self) -> Result<()> {
        use tokio::process::Command;
        
        // 执行任务命令
        let output = Command::new("sh")
            .arg("-c")
            .arg(&self.config.command)
            .output()
            .await
            .map_err(|e| NightError::Task(format!("Failed to execute command: {}", e)))?;
        
        // 检查命令执行结果
        if !output.status.success() {
            return Err(NightError::Task(format!(
                "Command '{}' failed with exit code: {}",
                self.config.command,
                output.status
            )));
        }
        
        Ok(())
    }

    pub async fn run_periodic(&self) -> Result<()> {
        let mut interval = interval(self.parse_interval()?);

        loop {
            interval.tick().await;
            if !self.execution_lock.load(Ordering::Relaxed) {
                break;
            }
            self.run_once().await?;
        }

        Ok(())
    }

    #[allow(dead_code)]
    async fn can_start(&self) -> bool {
        // 如果没有依赖，可以立即启动
        if self.dependency_status.lock().unwrap().is_empty() {
            return true;
        }
        
        let dependencies = self.dependency_status.lock().unwrap();
        dependencies.values().all(|&status| status)
    }

    async fn set_status(&self, new_status: TaskStatus) {
        let previous_status;
        {
            let mut status = self.status.lock().unwrap();
            previous_status = *status;
            *status = new_status;
        }
        
        // 发布任务状态变更事件
        self.event_system.publish(Event::task_status_changed(
            self.config.id,
            previous_status,
            new_status
        )).await.ok();
    }
    
    pub async fn set_status_external(&self, new_status: TaskStatus) {
        self.set_status(new_status).await;
    }

    async fn notify_completion(&self) -> Result<()> {
        // 发布任务完成事件
        let current_status = *self.status.lock().unwrap();
        match current_status {
            TaskStatus::Completed => {
                self.event_system.publish(Event::task_completed(self.config.id)).await?
            },
            TaskStatus::Failed => {
                self.event_system.publish(Event::task_failed(
                    self.config.id,
                    format!("Task {} failed", self.config.name)
                )).await?
            },
            _ => {}
        }
        
        Ok(())
    }

    pub async fn handle_dependency_completion(&self, completed_task_id: Uuid) -> Result<()> {
        let mut dependencies = self.dependency_status.lock().unwrap();
        if let Some(status) = dependencies.get_mut(&completed_task_id) {
            *status = true;
            println!(
                "Task {}: Dependency {} completed",
                self.config.name, completed_task_id
            );
        } else {
            println!(
                "Task {}: Received completion for non-dependency task {}",
                self.config.name, completed_task_id
            );
        }
        Ok(())
    }
    
    pub async fn setup_dependency_listeners(&self) -> Result<()> {
        // 为每个依赖任务设置完成事件监听器
        let task_name = self.config.name.clone();
        let dependency_status = self.dependency_status.clone();
        
        for &dep_id in self.config.dependencies.iter() {
            let dep_status = dependency_status.clone();
            let task_name = task_name.clone();
            
            let callback = Arc::new(move |event: Event| {
                let dep_status = dep_status.clone();
                let task_name = task_name.clone();
                
                let fut = async move {
                    if let Some(event_task_id) = event.task_id {
                        if event_task_id == dep_id {
                            let mut dependencies = dep_status.lock().unwrap();
                            if let Some(status) = dependencies.get_mut(&dep_id) {
                                *status = true;
                                println!("Task {}: Dependency {} completed", task_name, dep_id);
                            }
                        }
                    }
                    Ok(())
                };
                
                let boxed: Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>> = Box::pin(fut);
                boxed
            });
            
            let listener = crate::event::EventListener::new(
                EventType::TaskCompleted,
                Some(dep_id),
                callback
            );
            
            self.event_system.subscribe(listener).await?;
        }
        
        Ok(())
    }

    fn parse_interval(&self) -> Result<Duration> {
        // Parse the interval string into a Duration
        // For simplicity, let's assume it's always in milliseconds
        self.config
            .interval
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| NightError::Task("Invalid interval format".to_string()))
    }

    pub fn get_info(&self) -> TaskInfo {
        TaskInfo {
            id: self.config.id,
            status: *self.status.lock().unwrap(),
            start_time: *self.start_time.lock().unwrap(),
            end_time: *self.end_time.lock().unwrap(),
            execution_order: *self.execution_order.lock().unwrap(),
        }
    }

    pub fn set_dependency_status(&self, dependency_id: Uuid, status: bool) {
        let mut dependencies = self.dependency_status.lock().unwrap();
        if let Some(dep_status) = dependencies.get_mut(&dependency_id) {
            *dep_status = status;
        }
    }

    pub fn set_execution_lock(&self, status: bool) {
        self.execution_lock.store(status, Ordering::Relaxed);
    }
}
