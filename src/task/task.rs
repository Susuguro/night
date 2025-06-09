use crate::common::types::{DependencyState, TaskConfig, TaskDependencyMap, TaskInfo, TaskStatus};
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
use tokio::sync::Notify;
use tokio::time::{interval, Duration};
use uuid::Uuid;


static GLOBAL_EXECUTION_ORDER: AtomicUsize = AtomicUsize::new(0);

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
    pub notify_ready: Arc<Notify>,
}

impl Task {
    pub fn new(
        config: TaskConfig,
        event_system: Arc<EventSystem>,
        dependent_tasks: Vec<Uuid>,
    ) -> Self {
        let dependency_status: Arc<Mutex<TaskDependencyMap>> = Arc::new(Mutex::new(
            config
                .dependencies
                .iter()
                .map(|&id| (id, DependencyState::Pending))
                .collect(),
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
            notify_ready: Arc::new(Notify::new()),
        }
    }

    pub async fn run(&self) -> Result<()> {
        // Loop indefinitely until dependencies are met or task is stopped.
        loop {
            // 1. Check if the task has been externally stopped.
            if !self.execution_lock.load(Ordering::Relaxed) {
                println!("Task {}: Execution locked before starting (or during wait), not running.", self.config.name);
                return Ok(()); // Not an error, but task won't run.
            }

            // 2. Check if dependencies are met.
            if self.can_start().await {
                println!("Task {}: Dependencies met, proceeding to execution.", self.config.name);
                break; // Exit loop to proceed to task execution
            }

            // 3. Wait for a notification.
            println!("Task {}: Waiting for notification.", self.config.name); 
            self.notify_ready.notified().await;
            println!("Task {}: Notified, re-checking dependencies.", self.config.name); 
        }
        
        // At this point, dependencies are met and execution_lock was true when checked.
        // Re-check execution_lock one last time before actually running the command,
        // as a stop signal might have come in exactly after can_start() but before this point.
        if !self.execution_lock.load(Ordering::Relaxed) {
            println!("Task {}: Execution locked just before command execution, not running.", self.config.name);
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
                if !self.config.is_periodic {
                    // Re-check execution_lock before marking as Completed
                    if self.execution_lock.load(Ordering::Relaxed) {
                        self.set_status(TaskStatus::Completed).await;
                        println!("Task: Successfully completed {}", self.config.name);
                        self.notify_completion().await?;
                    } else {
                        // If lock became false during execution (due to stop_task),
                        // it should already be Pending. Do not override to Completed.
                        // It might already be Pending, or Running if stop_task hasn't fully updated it yet.
                        // The stop_task is responsible for setting it to Pending.
                        println!("Task: {} finished but was stopped during execution. Status remains as set by stop_task (likely Pending).", self.config.name);
                        // Ensure notify_completion still happens if it's a "successful" run that was just stopped.
                        // This depends on desired semantics - should a stopped task notify dependents in the same way?
                        // Current notify_completion logic sends TaskCompleted or TaskFailed.
                        // If it's Pending due to stop, maybe a different notification or none for "completion".
                        // For now, let's assume stop_task handles the final state and notifications related to stopping.
                        // If we want to ensure it's Pending here:
                        // self.set_status(TaskStatus::Pending).await; 
                        // However, stop_task should have already done this.
                    }
                }
                // Periodic tasks handle their own lifecycle and don't get Completed status here.
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
                println!("Task {}: Periodic execution stopped by lock.", self.config.name);
                break;
            }

            // println!("Task {}: Executing periodic run.", self.config.name); // Can be noisy
            match self.run_once().await {
                Ok(_) => {
                    // println!("Task {}: Periodic run_once completed successfully.", self.config.name); // Can be noisy
                    // Notify completion after each successful run_once for periodic tasks
                    self.event_system.publish(Event::task_completed(self.config.id.expect("Task ID should be present after config loading"))).await?;
                }
                Err(e) => {
                    // Log the error and continue the loop
                    println!("Task {}: Error in periodic run_once: {:?}. Continuing.", self.config.name, e);
                    // Also publish a failure event for this cycle
                    self.event_system.publish(Event::task_failed(
                        self.config.id.expect("Task ID should be present after config loading"),
                        format!("Periodic cycle failed: {:?}", e)
                    )).await?;
                }
            }
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
        // Check if any dependency has failed
        if dependencies
            .values()
            .any(|&state| state == DependencyState::Failed)
        {
            // If a task has a failed dependency, it should not start and its status should reflect this.
            // Consider setting self.status to TaskStatus::Failed or a new specific status here.
            // For now, just preventing start is the primary goal.
            // The run loop will keep it in Pending, or it might be set to Failed by another mechanism.
            println!(
                "Task {}: Cannot start due to a failed dependency.",
                self.config.name
            );
            return false;
        }
        // Otherwise, all dependencies must be Completed to start
        dependencies
            .values()
            .all(|&state| state == DependencyState::Completed)
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
            self.config.id.expect("Task ID should be present after config loading"),
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
                self.event_system.publish(Event::task_completed(self.config.id.expect("Task ID should be present after config loading"))).await?
            },
            TaskStatus::Failed => {
                self.event_system.publish(Event::task_failed(
                    self.config.id.expect("Task ID should be present after config loading"),
                    format!("Task {} failed", self.config.name)
                )).await?
            },
            _ => {}
        }
        
        Ok(())
    }

    // This method might be redundant if setup_dependency_listeners handles all state changes.
    // For now, let's assume it's not the primary path for dependency failure handling.
    // If it were to be used, it would need to accept DependencyState.
    // pub async fn handle_dependency_update(&self, dependency_id: Uuid, new_state: DependencyState) -> Result<()> {
    //     let mut dependencies = self.dependency_status.lock().unwrap();
    //     if let Some(status) = dependencies.get_mut(&dependency_id) {
    //         *status = new_state;
    //         println!(
    //             "Task {}: Dependency {} updated to {:?}",
    //             self.config.name, dependency_id, new_state
    //         );
    //     } else {
    //         println!(
    //             "Task {}: Received update for non-dependency task {}",
    //             self.config.name, dependency_id
    //         );
    //     }
    //     Ok(())
    // }

    pub async fn setup_dependency_listeners(&self) -> Result<()> {
        let task_id = self.config.id.expect("Task ID should be present after config loading"); // For logging/identification if needed inside callback
        let task_name = self.config.name.clone();
        let dependency_status_arc = self.dependency_status.clone();
        let notify_ready_arc = self.notify_ready.clone();
        // let status_arc = self.status.clone(); // To set self status to Failed/Blocked

        for &dep_id in self.config.dependencies.iter() {
            let dep_status_clone = dependency_status_arc.clone();
            let notify_clone = notify_ready_arc.clone();
            let task_name_clone = task_name.clone();
            // let self_status_clone = status_arc.clone();
            // let event_system_clone = self.event_system.clone(); // For publishing status change

            let callback = Arc::new(
                move |event: Event| -> Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>> {
                    let dep_status_cb = dep_status_clone.clone();
                    let notify_cb = notify_clone.clone();
                    let tn_cb = task_name_clone.clone();
                    // let own_status_cb = self_status_clone.clone();
                    // let es_cb = event_system_clone.clone();

                    Box::pin(async move {
                        if let Some(event_task_id) = event.task_id {
                            if event_task_id == dep_id {
                                let mut dependencies = dep_status_cb.lock().unwrap();
                                if let Some(current_dep_state) = dependencies.get_mut(&dep_id) {
                                    let new_dep_state = match event.event_type {
                                        EventType::TaskCompleted => DependencyState::Completed,
                                        EventType::TaskFailed => DependencyState::Failed,
                                        _ => return Ok(()), // Should not happen with current subscriptions
                                    };

                                    if *current_dep_state != new_dep_state {
                                        println!(
                                            "Task {}: Dependency {} transitioned to {:?}. Previous: {:?}",
                                            tn_cb, dep_id, new_dep_state, *current_dep_state
                                        );
                                        *current_dep_state = new_dep_state;

                                        // If a dependency fails, the current task itself is effectively blocked.
                                        // It should not run. The `can_start` check will handle this.
                                        // We might also want to change the task's own status here.
                                        // For example, if new_dep_state is Failed:
                                        // let mut current_task_status = own_status_cb.lock().unwrap();
                                        // if *current_task_status != TaskStatus::Failed && *current_task_status != TaskStatus::Completed {
                                        //     println!("Task {}: Moving to Pending/Blocked due to failed dependency {}", tn_cb, dep_id);
                                        //     // Or a new state like TaskStatus::BlockedByDependency
                                        //     // For now, let run() loop keep it Pending, can_start will prevent execution.
                                        // }
                                    }
                                    // Always notify, so can_start() is re-evaluated.
                                    // can_start() will then determine if it *actually* can proceed.
                                    notify_cb.notify_one();
                                }
                            }
                        }
                        Ok(())
                    })
                },
            );

            self.event_system
                .subscribe(crate::event::EventListener::new(
                    EventType::TaskCompleted,
                    Some(dep_id),
                    callback.clone(),
                ))
                .await?;

            self.event_system
                .subscribe(crate::event::EventListener::new(
                    EventType::TaskFailed,
                    Some(dep_id),
                    callback, // Same callback, it internally distinguishes by event.event_type
                ))
                .await?;
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
            id: self.config.id.expect("Task ID should be present after config loading"),
            status: *self.status.lock().unwrap(),
            start_time: *self.start_time.lock().unwrap(),
            end_time: *self.end_time.lock().unwrap(),
            execution_order: *self.execution_order.lock().unwrap(),
        }
    }

    // Updated to accept DependencyState
    pub fn set_dependency_status(&self, dependency_id: Uuid, state: DependencyState) {
        let mut dependencies = self.dependency_status.lock().unwrap();
        if let Some(dep_status) = dependencies.get_mut(&dependency_id) {
            *dep_status = state;
        }
    }

    pub fn set_execution_lock(&self, status: bool) {
        self.execution_lock.store(status, Ordering::Relaxed);
    }
}
