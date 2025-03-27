use crate::task::task::Task;
use crate::utils::error::Result;
use std::sync::Arc;
use tokio::time::{interval, Duration};

pub struct TaskScheduler {
    task: Arc<Task>,
}

impl TaskScheduler {
    pub fn new(task: Arc<Task>) -> Self {
        TaskScheduler { task }
    }

    pub async fn start(&self) -> Result<()> {
        if self.task.config.is_periodic {
            self.run_periodic().await
        } else {
            self.run_once().await
        }
    }

    async fn run_once(&self) -> Result<()> {
        self.task.run().await
    }

    async fn run_periodic(&self) -> Result<()> {
        let interval_duration = self.parse_interval()?;
        let mut interval = interval(interval_duration);

        loop {
            interval.tick().await;

            if !self.should_run() {
                break;
            }

            self.task.run().await?;
        }

        Ok(())
    }

    fn should_run(&self) -> bool {
        // Check if the task's execution lock is set
        self.task
            .execution_lock
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn parse_interval(&self) -> Result<Duration> {
        // Parse the interval string into a Duration
        self.task
            .config
            .interval
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| {
                crate::utils::error::NightError::Task("Invalid interval format".to_string())
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{common::types::{TaskConfig, TaskStatus}, event::EventSystem};
    use uuid::Uuid;

    fn create_test_task(is_periodic: bool, interval: &str) -> Arc<Task> {
        let config = TaskConfig {
            name: "Test Task".to_string(),
            id: Uuid::new_v4(),
            command: "echo Hello".to_string(),
            is_periodic,
            interval: interval.to_string(),
            importance: 1,
            dependencies: vec![],
        };
        // let address_map: Arc<HashMap<String, String>> = Arc::new(HashMap::new());
        let depend = config.dependencies.clone();
        Arc::new(Task::new(config, Arc::new(EventSystem::new()), depend))
    }

    #[tokio::test]
    async fn test_run_once() {
        let task = create_test_task(false, "0");
        let scheduler = TaskScheduler::new(task.clone());

        scheduler.start().await.unwrap();

        assert_eq!(*task.status.lock().unwrap(), TaskStatus::Completed);
    }

    #[tokio::test]
    async fn test_run_periodic() {
        let task = create_test_task(true, "100");
        let scheduler = TaskScheduler::new(task.clone());

        // Run the scheduler for a short time
        tokio::spawn(async move {
            scheduler.start().await.unwrap();
        });

        // Wait for a bit to allow multiple executions
        tokio::time::sleep(Duration::from_millis(350)).await;

        task.set_execution_lock(false);

        // Wait for the scheduler to stop
        tokio::time::sleep(Duration::from_millis(150)).await;

        assert_eq!(*task.status.lock().unwrap(), TaskStatus::Completed);
    }
}
