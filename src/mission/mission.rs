use crate::common::types::{MissionConfig, TaskInfo};
use crate::event::EventSystem;
use crate::mission::topology::TopologyManager;
use crate::common::types::DependencyState;
use crate::task::task::Task;
use crate::utils::error::{NightError, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use crate::common::types::TaskStatus;
use async_recursion::async_recursion;

#[derive(Clone)]
pub struct Mission {
    pub config: MissionConfig,
    pub topology: Arc<TopologyManager>,
    pub tasks: Arc<RwLock<HashMap<Uuid, Arc<Task>>>>,
    pub event_system: Arc<EventSystem>,
}

impl Mission {
    pub async fn new(config: MissionConfig) -> Result<Self> {
        let topology = Arc::new(TopologyManager::new(config.tasks.clone())?);
        let event_system = Arc::new(EventSystem::new());

        let mut dependent_tasks_map = HashMap::new();
        for task in &config.tasks {
            for &dep_id in &task.dependencies {
                dependent_tasks_map
                    .entry(dep_id)
                    .or_insert_with(Vec::new)
                    .push(task.id.expect("Task ID must be present when building dependency map"));
            }
        }

        let tasks = Arc::new(RwLock::new(HashMap::new()));
        for task_config in &config.tasks {
            let dependent_tasks = dependent_tasks_map
                .get(&task_config.id.expect("Task ID must be present when retrieving dependent tasks"))
                .cloned()
                .unwrap_or_default();
            let task = Arc::new(Task::new(
                task_config.clone(),
                event_system.clone(),
                dependent_tasks,
            ));
            tasks.write().await.insert(task_config.id.expect("ID should be present after config loading and validation"), task);
        }

        Ok(Mission {
            config,
            topology,
            tasks,
            event_system,
        })
    }

    pub async fn start(&self) -> Result<()> {
        // 设置所有任务的依赖监听器
        for (_, task) in self.tasks.read().await.iter() {
            task.setup_dependency_listeners().await?;
        }

        // After setting up all dependency listeners...
        println!("Mission: Notifying all tasks to perform initial checks.");
        for (_, task) in self.tasks.read().await.iter() {
            task.notify_ready.notify_one();
        }
        // The existing code that gets execution_order and iterates through levels to spawn tasks should follow.

        let execution_order = self.topology.get_execution_order();

        for (level, tasks) in execution_order.iter().enumerate() {
            println!("Mission: Starting execution of level {}", level);
            let mut handles = vec![];

            for &task_id in tasks {
                let task = self
                    .tasks
                    .read()
                    .await
                    .get(&task_id)
                    .cloned()
                    .ok_or_else(|| NightError::Mission(format!("Task {} not found", task_id)))?;

                println!(
                    "Mission: Scheduling task {} ({})",
                    task.config.name, task_id
                );
                let handle = tokio::spawn(async move { task.run().await });

                handles.push(handle);
            }

            // The following loop has been removed as per instructions:
            // for handle in handles {
            //     handle
            //         .await
            //         .map_err(|e| NightError::Mission(format!("Task execution failed: {}", e)))??;
            // }
            // `handles` vector is now populated but its elements are not awaited here.
            // The spawned tasks will run in the background.
        }

        Ok(())
    }

    pub async fn stop_task(&self, task_id: Uuid) -> Result<()> {
        let task = self
            .tasks
            .read()
            .await
            .get(&task_id)
            .cloned()
            .ok_or_else(|| NightError::Mission(format!("Task {} not found", task_id)))?;

        // 设置执行锁为false，阻止任务继续执行
        task.set_execution_lock(false);
        
        // 无论任务当前状态如何，都将其设置为Pending状态
        // 这样可以确保任务不会被标记为Completed
        task.set_status_external(TaskStatus::Pending).await;
        println!("Mission: Stopped task {} ({})", task.config.name, task_id);
        
        // 递归处理所有依赖于此任务的任务
        let mut visited = std::collections::HashSet::new();
        self.mark_dependent_tasks_incomplete(task_id, &mut visited).await?;
        
        Ok(())
    }
    
    // 递归地将所有依赖于指定任务的任务标记为不完整
    #[async_recursion]
    async fn mark_dependent_tasks_incomplete(&self, task_id: Uuid, visited: &mut std::collections::HashSet<Uuid>) -> Result<()> {
        // 防止循环依赖导致的无限递归
        if visited.contains(&task_id) {
            return Ok(());
        }
        visited.insert(task_id);
        
        // 获取任务和依赖任务ID列表
        let dependent_task_ids;
        {
            let tasks_read = self.tasks.read().await;
            let task = match tasks_read.get(&task_id) {
                Some(t) => t.clone(),
                None => return Ok(()), // 任务不存在，直接返回
            };
            dependent_task_ids = task.dependent_tasks.clone();
        } // 读锁在这里被释放
        
        // 对于每个依赖于此任务的任务
        for &dependent_id in dependent_task_ids.iter() {
            // 为每个依赖任务单独获取读锁
            let dependent_task_clone;
            {
                let tasks_read = self.tasks.read().await;
                let dependent_task = match tasks_read.get(&dependent_id) {
                    Some(t) => t.clone(),
                    None => continue, // 依赖任务不存在，继续下一个
                };
                
                // 将依赖状态设置为false，表示依赖未完成
                dependent_task.set_dependency_status(task_id, DependencyState::Pending);
                println!("Mission: Marked dependency {} as incomplete for task {}", 
                         task_id, dependent_id);
                
                // 将依赖任务的状态设置为Pending，防止它被执行
                dependent_task.set_execution_lock(false);
                dependent_task_clone = dependent_task;
            } // 读锁在这里被释放
            
            dependent_task_clone.set_status_external(TaskStatus::Pending).await;
            println!("Mission: Stopped dependent task {} ({})", dependent_task_clone.config.name, dependent_id);
            
            // 递归处理依赖于此依赖任务的任务
            self.mark_dependent_tasks_incomplete(dependent_id, visited).await?;
        }
        
        Ok(())
    }

    pub async fn get_task_info(&self, task_id: Uuid) -> Result<TaskInfo> {
        let task = self
            .tasks
            .read()
            .await
            .get(&task_id)
            .cloned()
            .ok_or_else(|| NightError::Mission(format!("Task {} not found", task_id)))?;

        Ok(task.get_info())
    }

    pub async fn get_all_task_info(&self) -> HashMap<Uuid, TaskInfo> {
        let tasks = self.tasks.read().await;
        tasks
            .iter()
            .map(|(&id, task)| (id, task.get_info()))
            .collect()
    }

    pub fn get_event_system(&self) -> Arc<EventSystem> {
        self.event_system.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::{TaskConfig, TaskStatus};
    use tokio::time::{sleep, Duration}; // Added import

    fn create_test_config() -> MissionConfig {
        MissionConfig {
            name: "Test Mission".to_string(),
            tasks: vec![
                TaskConfig {
                    name: "Task 1".to_string(),
                    id: Some(Uuid::new_v4()),
                    command: "echo Task 1".to_string(), // Simple, fast command
                    is_periodic: false,
                    interval: "0".to_string(),
                    importance: 1,
                    dependencies: vec![],
                },
                TaskConfig {
                    name: "Task 2".to_string(),
                    id: Some(Uuid::new_v4()),
                    command: "echo Task 2".to_string(), // Simple, fast command
                    is_periodic: false,
                    interval: "0".to_string(),
                    importance: 1,
                    dependencies: vec![],
                },
            ],
        }
    }

    #[tokio::test]
    async fn test_mission_creation() {
        let config = create_test_config();
        let mission = Mission::new(config).await;
        assert!(mission.is_ok());
    }

    #[tokio::test]
    async fn test_mission_execution() {
        let config = create_test_config();
        let num_tasks = config.tasks.len();
        let mission = Mission::new(config).await.unwrap();
        let result = mission.start().await;
        assert!(result.is_ok());

        let max_attempts = 30; // e.g., 30 * 100ms = 3 seconds timeout
        let mut all_completed = false;
        for _ in 0..max_attempts {
            let task_info_map = mission.get_all_task_info().await;
            let completed_count = task_info_map
                .values()
                .filter(|info| info.status == TaskStatus::Completed)
                .count();

            if completed_count == num_tasks {
                all_completed = true;
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }

        // Assertions remain the same; they will fail if not all_completed after the loop.
        let final_task_info = mission.get_all_task_info().await;
        for (id, info) in final_task_info {
            assert_eq!(info.status, TaskStatus::Completed, "Task {} did not complete. Status: {:?}", id, info.status);
        }
        assert!(all_completed, "Not all tasks completed within the timeout."); // Extra assertion for clarity
    }

    #[tokio::test]
    async fn test_stop_task() {
        let config = create_test_config();
        let mission = Mission::new(config).await.unwrap();

        let task_id = mission.config.tasks[0].id.expect("Test task should have an ID");
        let result = mission.stop_task(task_id).await;
        assert!(result.is_ok());

        let task_info = mission.get_task_info(task_id).await.unwrap();
        assert_eq!(task_info.status, TaskStatus::Pending);
    }
}
