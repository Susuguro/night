use crate::task::task::Task;
use crate::utils::error::{NightError, Result}; // Added NightError for format!
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
        // Perform initial run if should_run is true
        if self.should_run() {
            self.task.run().await?;
        } else {
            // If not allowed to run initially, just return.
            // This prevents starting the interval timer if the task is immediately cancelled.
            return Ok(());
        }

        let interval_duration = self.parse_interval()?;
        let mut interval = interval(interval_duration);

        loop {
            // Wait for the next interval tick.
            interval.tick().await;

            // Check if the task should continue running before each execution.
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

    pub fn parse_interval(&self) -> Result<Duration> {
        let interval_str = &self.task.config.interval;
        interval_str
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| {
                NightError::Task(format!("Invalid interval format: '{}'", interval_str))
            })
    }
}
