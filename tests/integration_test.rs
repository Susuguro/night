use night::common::types::{MissionConfig, TaskConfig, TaskStatus};
use night::{get_all_task_info, get_task_info, init, run, stop_task, Mission, Result};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tempfile::TempDir;
use tokio;
use tokio::time::Duration;
use uuid::Uuid;

// ===== Test Fixtures =====

/// A test fixture for mission tests
struct MissionFixture {
    mission: Mission,
    task_ids: HashMap<char, Uuid>,
    _temp_dir: TempDir, // Keep the temp dir alive for the test duration
}

impl MissionFixture {
    /// Create a new mission fixture with the given configuration
    async fn new(config: MissionConfig, task_ids: HashMap<char, Uuid>) -> Result<Self> {
        let (temp_dir, config_path) = create_temp_config_file(&config)?;
        let mission = init(config_path.to_str().unwrap()).await?;
        Ok(MissionFixture {
            mission,
            task_ids,
            _temp_dir: temp_dir,
        })
    }

    /// Create a new mission fixture with a standard test configuration
    async fn new_standard() -> Result<Self> {
        let (config, task_ids) = create_complex_test_config();
        Self::new(config, task_ids).await
    }

    /// Run the mission and wait for all tasks to complete
    async fn run_and_wait(&self, timeout: Duration) -> Result<()> {
        // Start the mission in a separate task
        let mission_clone = self.mission.clone();
        tokio::spawn(async move {
            run(&mission_clone).await.unwrap();
        });

        // Wait for all tasks to complete
        wait_for_tasks_completion(&self.mission, &self.task_ids, timeout).await
    }

    /// Get the execution order of a task by its identifier
    async fn get_execution_order(&self, task_char: char) -> Result<usize> {
        let task_id = self.task_ids[&task_char];
        let info = get_task_info(&self.mission, task_id).await?;
        Ok(info.execution_order.unwrap_or(0))
    }

    /// Get the status of a task by its identifier
    async fn get_task_status(&self, task_char: char) -> Result<TaskStatus> {
        let task_id = self.task_ids[&task_char];
        let info = get_task_info(&self.mission, task_id).await?;
        Ok(info.status)
    }
}

// ===== Helper Functions =====

/// Create a temporary config file from a mission configuration
fn create_temp_config_file(config: &MissionConfig) -> Result<(TempDir, PathBuf)> {
    let temp_dir = tempfile::tempdir()?;
    let config_path = temp_dir.path().join("config.json");
    let config_json = serde_json::to_string_pretty(&config)?;
    fs::write(&config_path, config_json)?;
    Ok((temp_dir, config_path))
}

/// Create a complex test configuration with interdependent tasks
fn create_complex_test_config() -> (MissionConfig, HashMap<char, Uuid>) {
    let mut task_ids = HashMap::new();
    let task_names = vec!['A', 'B', 'C', 'D', 'E'];

    for &name in &task_names {
        task_ids.insert(name, Uuid::new_v4());
    }

    let tasks = vec![
        TaskConfig {
            name: "Task A".to_string(),
            id: task_ids[&'A'],
            command: "echo Task A".to_string(),
            is_periodic: false,
            interval: "1".to_string(),
            importance: 1,
            dependencies: vec![],
        },
        TaskConfig {
            name: "Task B".to_string(),
            id: task_ids[&'B'],
            command: "echo Task B".to_string(),
            is_periodic: false,
            interval: "1".to_string(),
            importance: 1,
            dependencies: vec![task_ids[&'A']],
        },
        TaskConfig {
            name: "Task C".to_string(),
            id: task_ids[&'C'],
            command: "echo Task C".to_string(),
            is_periodic: false,
            interval: "1".to_string(),
            importance: 1,
            dependencies: vec![task_ids[&'B'], task_ids[&'E']],
        },
        TaskConfig {
            name: "Task D".to_string(),
            id: task_ids[&'D'],
            command: "echo Task D".to_string(),
            is_periodic: false,
            interval: "1".to_string(),
            importance: 1,
            dependencies: vec![task_ids[&'C']],
        },
        TaskConfig {
            name: "Task E".to_string(),
            id: task_ids[&'E'],
            command: "echo Task E".to_string(),
            is_periodic: false,
            interval: "1".to_string(),
            importance: 1,
            dependencies: vec![task_ids[&'A']],
        },
    ];

    (
        MissionConfig {
            name: "Complex Test Mission".to_string(),
            tasks,
        },
        task_ids,
    )
}

/// Create a configuration with a failing task
fn create_failing_task_config() -> (MissionConfig, HashMap<char, Uuid>) {
    let mut task_ids = HashMap::new();
    let task_names = vec!['A', 'B', 'C'];

    for &name in &task_names {
        task_ids.insert(name, Uuid::new_v4());
    }

    let tasks = vec![
        TaskConfig {
            name: "Task A".to_string(),
            id: task_ids[&'A'],
            command: "echo Task A".to_string(),
            is_periodic: false,
            interval: "1".to_string(),
            importance: 1,
            dependencies: vec![],
        },
        TaskConfig {
            name: "Failing Task B".to_string(),
            id: task_ids[&'B'],
            command: "exit 1".to_string(), // This command will fail
            is_periodic: false,
            interval: "1".to_string(),
            importance: 1,
            dependencies: vec![task_ids[&'A']],
        },
        TaskConfig {
            name: "Task C".to_string(),
            id: task_ids[&'C'],
            command: "echo Task C".to_string(),
            is_periodic: false,
            interval: "1".to_string(),
            importance: 1,
            dependencies: vec![task_ids[&'B']], // This depends on the failing task
        },
    ];

    (
        MissionConfig {
            name: "Failing Task Mission".to_string(),
            tasks,
        },
        task_ids,
    )
}

/// Create a configuration with periodic tasks
fn create_periodic_task_config() -> (MissionConfig, HashMap<char, Uuid>) {
    let mut task_ids = HashMap::new();
    let task_names = vec!['A', 'B'];

    for &name in &task_names {
        task_ids.insert(name, Uuid::new_v4());
    }

    let tasks = vec![
        TaskConfig {
            name: "Periodic Task A".to_string(),
            id: task_ids[&'A'],
            command: "echo Periodic Task A".to_string(),
            is_periodic: true,
            interval: "100".to_string(), // 100ms interval
            importance: 1,
            dependencies: vec![],
        },
        TaskConfig {
            name: "Task B".to_string(),
            id: task_ids[&'B'],
            command: "echo Task B".to_string(),
            is_periodic: false,
            interval: "1".to_string(),
            importance: 1,
            dependencies: vec![],
        },
    ];

    (
        MissionConfig {
            name: "Periodic Task Mission".to_string(),
            tasks,
        },
        task_ids,
    )
}

/// Wait for tasks to complete with a timeout
async fn wait_for_tasks_completion(
    mission: &Mission,
    task_ids: &HashMap<char, Uuid>,
    max_wait: Duration,
) -> Result<()> {
    let start = Instant::now();
    while start.elapsed() < max_wait {
        let mut all_completed = true;
        for (&task_char, &id) in task_ids {
            match get_task_info(mission, id).await {
                Ok(info) => {
                    log::debug!("Task {} ({}): status: {:?}", task_char, id, info.status);
                    if info.status != TaskStatus::Completed && info.status != TaskStatus::Failed {
                        all_completed = false;
                        break;
                    }
                }
                Err(e) => {
                    log::error!(
                        "Failed to get info for task {} ({}): {:?}",
                        task_char,
                        id,
                        e
                    );
                    all_completed = false;
                    break;
                }
            }
        }

        if all_completed {
            return Ok(());
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}

// ===== Tests =====

#[tokio::test]
async fn test_mission_initialization() -> Result<()> {
    let fixture = MissionFixture::new_standard().await?;
    
    // Verify mission configuration
    assert_eq!(fixture.mission.config.name, "Complex Test Mission");
    assert_eq!(fixture.mission.config.tasks.len(), 5);
    
    Ok(())
}

#[tokio::test]
async fn test_mission_execution() -> Result<()> {
    let fixture = MissionFixture::new_standard().await?;
    
    // Run the mission and wait for completion
    fixture.run_and_wait(Duration::from_secs(10)).await?;
    
    // Check if all tasks are completed
    for task_char in ['A', 'B', 'C', 'D', 'E'] {
        let status = fixture.get_task_status(task_char).await?;
        assert_eq!(status, TaskStatus::Completed, "Task {} did not complete", task_char);
    }
    
    // Verify execution order
    let a_order = fixture.get_execution_order('A').await?;
    let b_order = fixture.get_execution_order('B').await?;
    let c_order = fixture.get_execution_order('C').await?;
    let d_order = fixture.get_execution_order('D').await?;
    let e_order = fixture.get_execution_order('E').await?;
    
    // Check execution order constraints
    assert!(a_order < b_order, "Task A should execute before Task B");
    assert!(a_order < e_order, "Task A should execute before Task E");
    assert!(b_order < c_order, "Task B should execute before Task C");
    assert!(e_order < c_order, "Task E should execute before Task C");
    assert!(c_order < d_order, "Task C should execute before Task D");
    
    Ok(())
}

#[tokio::test]
async fn test_failing_task() -> Result<()> {
    let (config, task_ids) = create_failing_task_config();
    let fixture = MissionFixture::new(config, task_ids).await?;
    
    // Run the mission and wait for completion
    fixture.run_and_wait(Duration::from_secs(10)).await?;
    
    // Task A should complete successfully
    let a_status = fixture.get_task_status('A').await?;
    assert_eq!(a_status, TaskStatus::Completed, "Task A should complete");
    
    // Task B should fail
    let b_status = fixture.get_task_status('B').await?;
    assert_eq!(b_status, TaskStatus::Failed, "Task B should fail");
    
    // Task C should not run because its dependency failed
    let c_status = fixture.get_task_status('C').await?;
    assert_eq!(c_status, TaskStatus::Pending, "Task C should remain pending");
    
    Ok(())
}

#[tokio::test]
async fn test_stop_task() -> Result<()> {
    let fixture = MissionFixture::new_standard().await?;
    
    // Start the mission
    let mission_clone = fixture.mission.clone();
    tokio::spawn(async move {
        run(&mission_clone).await.unwrap();
    });
    
    // Wait a bit for the mission to start
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Stop task B
    let task_b_id = fixture.task_ids[&'B'];
    stop_task(&fixture.mission, task_b_id).await?;
    
    // Wait for other tasks to complete
    wait_for_tasks_completion(&fixture.mission, &fixture.task_ids, Duration::from_secs(5)).await?;
    
    // Task A should complete
    let a_status = fixture.get_task_status('A').await?;
    assert_eq!(a_status, TaskStatus::Completed, "Task A should complete");
    
    // Task B should not complete (it was stopped)
    let b_status = fixture.get_task_status('B').await?;
    assert_ne!(b_status, TaskStatus::Completed, "Task B should not complete");
    
    // Tasks C and D should not complete (they depend on B)
    let c_status = fixture.get_task_status('C').await?;
    assert_ne!(c_status, TaskStatus::Completed, "Task C should not complete");
    
    let d_status = fixture.get_task_status('D').await?;
    assert_ne!(d_status, TaskStatus::Completed, "Task D should not complete");
    
    // Task E should complete (it only depends on A)
    let e_status = fixture.get_task_status('E').await?;
    assert_eq!(e_status, TaskStatus::Completed, "Task E should complete");
    
    Ok(())
}

#[tokio::test]
async fn test_periodic_task() -> Result<()> {
    let (config, task_ids) = create_periodic_task_config();
    let fixture = MissionFixture::new(config, task_ids).await?;
    
    // Start the mission
    let mission_clone = fixture.mission.clone();
    // let execution_counter = Arc::new(Mutex::new(0));
    
    // Create a counter to track how many times the periodic task runs
    // let counter_clone = execution_counter.clone();
    tokio::spawn(async move {
        // Start the mission
        run(&mission_clone).await.unwrap();
    });
    
    // Wait a bit to allow the periodic task to execute multiple times
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Get all task info
    let all_info = get_all_task_info(&fixture.mission).await;
    
    // Task B should be completed
    let b_info = all_info.get(&fixture.task_ids[&'B']).unwrap();
    assert_eq!(b_info.status, TaskStatus::Completed, "Task B should complete");
    
    // Stop the periodic task
    stop_task(&fixture.mission, fixture.task_ids[&'A']).await?;
    
    // Wait for the task to stop
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    // Get the final task info
    let a_info = get_task_info(&fixture.mission, fixture.task_ids[&'A']).await?;
    
    // The periodic task should have run at least once
    assert!(a_info.execution_order.is_some(), "Periodic task should have run at least once");
    
    Ok(())
}

#[tokio::test]
async fn test_task_info_retrieval() -> Result<()> {
    let fixture = MissionFixture::new_standard().await?;
    
    // Start the mission and wait for completion
    fixture.run_and_wait(Duration::from_secs(10)).await?;
    
    // Get all task info
    let all_info = get_all_task_info(&fixture.mission).await;
    
    // Verify we have info for all tasks
    assert_eq!(all_info.len(), 5, "Should have info for all 5 tasks");
    
    // Verify each task has the correct status
    for (_, info) in all_info {
        assert_eq!(info.status, TaskStatus::Completed, "All tasks should be completed");
        assert!(info.start_time.is_some(), "Start time should be recorded");
        assert!(info.end_time.is_some(), "End time should be recorded");
        assert!(info.execution_order.is_some(), "Execution order should be recorded");
    }
    
    Ok(())
}
