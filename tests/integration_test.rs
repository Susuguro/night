use night::common::types::{MissionConfig, TaskConfig, TaskStatus};
use night::{get_all_task_info, get_task_info, init, run, stop_task, Mission, Result, NightError};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tempfile::TempDir;
use tokio;
use tokio::time::Duration;
use uuid::Uuid;

mod task;
mod mission;

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
        let mission = init(config_path.to_str().unwrap()).await?; // unwrap is okay for test setup
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

    /// Run the mission and wait for all tasks to complete or fail
    async fn run_and_wait_for_all(&self, timeout: Duration) -> Result<()> {
        let mission_clone = self.mission.clone();
        let mission_handle = tokio::spawn(async move {
            match run(&mission_clone).await {
                Ok(_) => {
                    log::info!("Mission execution completed successfully.");
                    Ok(()) // Explicitly return Ok for the handle's result
                }
                Err(e) => {
                    log::error!("Mission execution failed: {:?}", e);
                    Err(e) // Propagate the error
                }
            }
        });

        // Wait for the mission itself to finish, or timeout
        match tokio::time::timeout(timeout, mission_handle).await {
            Ok(Ok(mission_result)) => { // Mission handle finished, and the inner result from run
                if let Err(e) = mission_result {
                     log::warn!("Mission run returned an error: {:?}", e);
                }
            }
            Ok(Err(join_error)) => { // Mission handle panicked
                return Err(NightError::Mission(format!("Mission task panicked: {:?}", join_error)));
            }
            Err(_) => { // Timeout
                log::warn!("Mission execution timed out after {:?}.", timeout);
            }
        }
        
        wait_for_tasks_to_settle(&self.mission, &self.task_ids.values().cloned().collect(), Duration::from_millis(500)).await 
    }
}

// ===== Helper Functions =====

fn create_temp_config_file(config: &MissionConfig) -> Result<(TempDir, PathBuf)> {
    let temp_dir = tempfile::tempdir()?;
    let config_path = temp_dir.path().join("config.json");
    let config_json = serde_json::to_string_pretty(&config)?;
    fs::write(&config_path, config_json)?;
    Ok((temp_dir, config_path))
}

async fn wait_for_tasks_to_settle(
    mission: &Mission,
    task_ids: &Vec<Uuid>,
    max_wait: Duration,
) -> Result<()> {
    let start = Instant::now();
    while start.elapsed() < max_wait {
        let mut all_settled = true;
        for &id in task_ids {
            match get_task_info(mission, id).await {
                Ok(info) => {
                    if info.status == TaskStatus::Running { 
                        all_settled = false;
                        break;
                    }
                }
                Err(_) => { 
                    all_settled = false;
                    break;
                }
            }
        }
        if all_settled { return Ok(()); }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    log::warn!("wait_for_tasks_to_settle timed out after {:?}.", max_wait);
    Ok(())
}

fn create_complex_test_config() -> (MissionConfig, HashMap<char, Uuid>) {
    let mut task_ids = HashMap::new();
    let task_names = vec!['A', 'B', 'C', 'D', 'E'];
    for &name in &task_names { task_ids.insert(name, Uuid::new_v4()); }
    let tasks = vec![
        TaskConfig { name: "Task A".to_string(), id: task_ids[&'A'], command: "echo Task A".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![] },
        TaskConfig { name: "Task B".to_string(), id: task_ids[&'B'], command: "echo Task B".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![task_ids[&'A']] },
        TaskConfig { name: "Task C".to_string(), id: task_ids[&'C'], command: "echo Task C".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![task_ids[&'B'], task_ids[&'E']] },
        TaskConfig { name: "Task D".to_string(), id: task_ids[&'D'], command: "echo Task D".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![task_ids[&'C']] },
        TaskConfig { name: "Task E".to_string(), id: task_ids[&'E'], command: "echo Task E".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![task_ids[&'A']] },
    ];
    (MissionConfig { name: "Complex Test Mission".to_string(), tasks }, task_ids)
}

fn create_simple_a_b_config() -> (MissionConfig, HashMap<char, Uuid>) {
    let mut task_ids = HashMap::new();
    task_ids.insert('A', Uuid::new_v4()); task_ids.insert('B', Uuid::new_v4());
    let tasks = vec![
        TaskConfig { name: "Task A".to_string(), id: task_ids[&'A'], command: "echo Task A complete".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![] },
        TaskConfig { name: "Task B".to_string(), id: task_ids[&'B'], command: "echo Task B complete".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![task_ids[&'A']] },
    ];
    (MissionConfig { name: "Simple A B Mission".to_string(), tasks }, task_ids)
}

fn create_periodic_a_periodic_b_config(
    task_a_output_path: String, 
    task_b_output_path: String, 
    task_b_signal_path: String, 
) -> (MissionConfig, HashMap<char, Uuid>) {
    let mut task_ids = HashMap::new();
    task_ids.insert('A', Uuid::new_v4()); 
    task_ids.insert('B', Uuid::new_v4()); 

    let task_a_command = format!("echo Periodic A running >> \"{}\"", task_a_output_path);
    let task_b_command = format!(
        "echo Periodic B running >> \"{}\"; \
        if [ ! -f \"{}\" ]; then \
            touch \"{}\" && echo 'Signal created by Task B' >> \"{}\"; \
        fi",
        task_b_output_path, 
        task_b_signal_path, task_b_signal_path, task_b_signal_path
    );

    let tasks = vec![
        TaskConfig {
            name: "Periodic Task A".to_string(), id: task_ids[&'A'], command: task_a_command,
            is_periodic: true, interval: "100".to_string(), importance: 1, dependencies: vec![],
        },
        TaskConfig {
            name: "Periodic Task B".to_string(), id: task_ids[&'B'], command: task_b_command,
            is_periodic: true, interval: "100".to_string(), importance: 1, dependencies: vec![task_ids[&'A']],
        },
    ];
    (MissionConfig { name: "Periodic A Periodic B Mission".to_string(), tasks }, task_ids)
}

fn create_normal_a_periodic_b_config() -> (MissionConfig, HashMap<char, Uuid>) {
    let mut task_ids = HashMap::new();
    task_ids.insert('A', Uuid::new_v4()); task_ids.insert('B', Uuid::new_v4());
    let tasks = vec![
        TaskConfig { name: "Normal Task A".to_string(), id: task_ids[&'A'], command: "sleep 0.1; echo Normal A completed".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![] },
        TaskConfig { name: "Periodic Task B".to_string(), id: task_ids[&'B'], command: "echo Periodic B running".to_string(), is_periodic: true, interval: "100".to_string(), importance: 1, dependencies: vec![task_ids[&'A']] },
    ];
    (MissionConfig { name: "Normal A Periodic B Mission".to_string(), tasks }, task_ids)
}

fn create_periodic_a_normal_b_config() -> (MissionConfig, HashMap<char, Uuid>) {
    let mut task_ids = HashMap::new();
    task_ids.insert('A', Uuid::new_v4()); task_ids.insert('B', Uuid::new_v4());
    let tasks = vec![
        TaskConfig { name: "Periodic Task A".to_string(), id: task_ids[&'A'], command: "echo Periodic A running".to_string(), is_periodic: true, interval: "100".to_string(), importance: 1, dependencies: vec![] },
        TaskConfig { name: "Normal Task B".to_string(), id: task_ids[&'B'], command: "echo Normal B running".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![task_ids[&'A']] },
    ];
    (MissionConfig { name: "Periodic A Normal B Mission".to_string(), tasks }, task_ids)
}

fn create_fan_in_dependency_config() -> (MissionConfig, HashMap<char, Uuid>) {
    let mut task_ids = HashMap::new();
    let task_names = vec!['A', 'B', 'C'];
    for &name in &task_names { task_ids.insert(name, Uuid::new_v4()); }
    let tasks = vec![
        TaskConfig { name: "Task A".to_string(), id: task_ids[&'A'], command: "sleep 0.5; echo Task A".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![] },
        TaskConfig { name: "Task B".to_string(), id: task_ids[&'B'], command: "sleep 0.5; echo Task B".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![] },
        TaskConfig { name: "Task C".to_string(), id: task_ids[&'C'], command: "sleep 0.5; echo Task C".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![task_ids[&'A'], task_ids[&'B']] },
    ];
    (MissionConfig { name: "Fan-in Dependency Mission".to_string(), tasks }, task_ids)
}

fn create_fan_out_dependency_config() -> (MissionConfig, HashMap<char, Uuid>) {
    let mut task_ids = HashMap::new();
    let task_names = vec!['A', 'B', 'C', 'D'];
    for &name in &task_names { task_ids.insert(name, Uuid::new_v4()); }
    let tasks = vec![
        TaskConfig { name: "Task A".to_string(), id: task_ids[&'A'], command: "sleep 0.5; echo Task A".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![] },
        TaskConfig { name: "Task B".to_string(), id: task_ids[&'B'], command: "sleep 0.5; echo Task B".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![task_ids[&'A']] },
        TaskConfig { name: "Task C".to_string(), id: task_ids[&'C'], command: "sleep 0.5; echo Task C".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![task_ids[&'A']] },
        TaskConfig { name: "Task D".to_string(), id: task_ids[&'D'], command: "sleep 0.5; echo Task D".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![task_ids[&'A']] },
    ];
    (MissionConfig { name: "Fan-out Dependency Mission".to_string(), tasks }, task_ids)
}

fn create_chained_dependency_config() -> (MissionConfig, HashMap<char, Uuid>) {
    let mut task_ids = HashMap::new();
    let task_names = vec!['A', 'B', 'C', 'D'];
    for &name in &task_names { task_ids.insert(name, Uuid::new_v4()); }
    let tasks = vec![
        TaskConfig { name: "Task A".to_string(), id: task_ids[&'A'], command: "sleep 0.5; echo Task A".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![] },
        TaskConfig { name: "Task B".to_string(), id: task_ids[&'B'], command: "sleep 0.5; echo Task B".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![task_ids[&'A']] },
        TaskConfig { name: "Task C".to_string(), id: task_ids[&'C'], command: "sleep 0.5; echo Task C".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![task_ids[&'B']] },
        TaskConfig { name: "Task D".to_string(), id: task_ids[&'D'], command: "sleep 0.5; echo Task D".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![task_ids[&'C']] },
    ];
    (MissionConfig { name: "Chained Dependency Mission".to_string(), tasks }, task_ids)
}

fn create_failing_task_config() -> (MissionConfig, HashMap<char, Uuid>) {
    let mut task_ids = HashMap::new();
    let task_names = vec!['A', 'B', 'C'];
    for &name in &task_names { task_ids.insert(name, Uuid::new_v4()); }
    let tasks = vec![
        TaskConfig { name: "Task A".to_string(), id: task_ids[&'A'], command: "sleep 0.5; echo Task A".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![] },
        TaskConfig { name: "Failing Task B".to_string(), id: task_ids[&'B'], command: "asdfghjkl_nonexistent_command".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![task_ids[&'A']] },
        TaskConfig { name: "Task C".to_string(), id: task_ids[&'C'], command: "sleep 0.5; echo Task C".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![task_ids[&'A'], task_ids[&'B']] },
    ];
    (MissionConfig { name: "Failing Task Mission".to_string(), tasks }, task_ids)
}

fn create_periodic_task_config() -> (MissionConfig, HashMap<char, Uuid>) {
    let mut task_ids = HashMap::new();
    task_ids.insert('A', Uuid::new_v4()); task_ids.insert('B', Uuid::new_v4());
    let tasks = vec![
        TaskConfig { name: "Periodic Task A".to_string(), id: task_ids[&'A'], command: "echo Periodic Task A".to_string(), is_periodic: true, interval: "100".to_string(), importance: 1, dependencies: vec![] },
        TaskConfig { name: "Task B".to_string(), id: task_ids[&'B'], command: "echo Task B".to_string(), is_periodic: false, interval: "1".to_string(), importance: 1, dependencies: vec![] },
    ];
    (MissionConfig { name: "Periodic Task Mission".to_string(), tasks }, task_ids)
}


// ===== Tests =====

#[tokio::test]
async fn test_mission_execution_with_yaml_config() -> Result<()> {
    let config_path = "tests/fixtures/test_config.yaml"; 
    let mission = init(config_path).await?;
    assert_eq!(mission.config.name, "YAML Test Mission");
    assert_eq!(mission.config.tasks.len(), 3);
    let task_a_id = mission.config.tasks.iter().find(|t| t.name == "Task A YAML").unwrap().id;
    let task_b_id = mission.config.tasks.iter().find(|t| t.name == "Task B YAML").unwrap().id;
    let task_c_id = mission.config.tasks.iter().find(|t| t.name == "Task C YAML").unwrap().id;
    let mission_clone = mission.clone();
    tokio::spawn(async move { match run(&mission_clone).await { Ok(_) => log::info!("YAML Mission completed."), Err(e) => log::error!("YAML Mission failed: {:?}", e), } });
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(get_task_info(&mission, task_a_id).await?.status, TaskStatus::Completed, "Task A YAML should complete");
    assert_eq!(get_task_info(&mission, task_b_id).await?.status, TaskStatus::Completed, "Task B YAML should complete");
    assert_eq!(get_task_info(&mission, task_c_id).await?.status, TaskStatus::Completed, "Task C YAML should complete");
    Ok(())
}

#[tokio::test]
async fn test_mission_initialization() -> Result<()> {
    let fixture = MissionFixture::new_standard().await?;
    assert_eq!(fixture.mission.config.name, "Complex Test Mission");
    assert_eq!(fixture.mission.config.tasks.len(), 5);
    Ok(())
}

#[tokio::test]
async fn test_mission_execution() -> Result<()> {
    let fixture = MissionFixture::new_standard().await?;
    fixture.run_and_wait_for_all(Duration::from_secs(10)).await?;
    for task_char in ['A', 'B', 'C', 'D', 'E'] {
        let status = fixture.mission.get_task_info(fixture.task_ids[&task_char]).await?.status;
        assert_eq!(status, TaskStatus::Completed, "Task {} did not complete", task_char);
    }
    let a_order = fixture.mission.get_task_info(fixture.task_ids[&'A']).await?.execution_order.unwrap_or(0);
    let b_order = fixture.mission.get_task_info(fixture.task_ids[&'B']).await?.execution_order.unwrap_or(0);
    let c_order = fixture.mission.get_task_info(fixture.task_ids[&'C']).await?.execution_order.unwrap_or(0);
    let d_order = fixture.mission.get_task_info(fixture.task_ids[&'D']).await?.execution_order.unwrap_or(0);
    let e_order = fixture.mission.get_task_info(fixture.task_ids[&'E']).await?.execution_order.unwrap_or(0);
    assert!(a_order < b_order, "A before B"); assert!(a_order < e_order, "A before E");
    assert!(b_order < c_order, "B before C"); assert!(e_order < c_order, "E before C");
    assert!(c_order < d_order, "C before D");
    Ok(())
}

#[tokio::test]
async fn test_failing_task() -> Result<()> {
    let (config, task_ids) = create_failing_task_config();
    let fixture = MissionFixture::new(config, task_ids).await?;
    fixture.run_and_wait_for_all(Duration::from_secs(10)).await?;
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'A']).await?.status, TaskStatus::Completed, "Task A should complete");
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'B']).await?.status, TaskStatus::Failed, "Task B should fail");
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'C']).await?.status, TaskStatus::Pending, "Task C should be Pending");
    Ok(())
}

#[tokio::test]
async fn test_stop_task() -> Result<()> { 
    let fixture = MissionFixture::new_standard().await?;
    let mission_clone = fixture.mission.clone();
    tokio::spawn(async move { run(&mission_clone).await.unwrap(); });
    tokio::time::sleep(Duration::from_millis(100)).await; 
    stop_task(&fixture.mission, fixture.task_ids[&'B']).await?;
    wait_for_tasks_to_settle(&fixture.mission, &fixture.task_ids.values().cloned().collect(), Duration::from_secs(5)).await?;
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'A']).await?.status, TaskStatus::Completed, "Task A should complete");
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'B']).await?.status, TaskStatus::Pending, "Task B should be Pending");
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'C']).await?.status, TaskStatus::Pending, "Task C should be Pending");
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'D']).await?.status, TaskStatus::Pending, "Task D should be Pending");
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'E']).await?.status, TaskStatus::Completed, "Task E should complete");
    Ok(())
}

#[tokio::test]
async fn test_periodic_task_execution_and_stop() -> Result<()> {
    let temp_dir_task = tempfile::tempdir()?;
    let output_file_path = temp_dir_task.path().join("periodic_output.txt");
    let (mut config, task_ids) = create_periodic_task_config();
    if let Some(task_a_config) = config.tasks.iter_mut().find(|t| t.id == task_ids[&'A']) {
        task_a_config.command = format!("echo 'Periodic execution' >> {}", output_file_path.display());
    }
    let fixture = MissionFixture::new(config, task_ids.clone()).await?;
    let mission_clone = fixture.mission.clone();
    tokio::spawn(async move { run(&mission_clone).await.unwrap(); });
    tokio::time::sleep(Duration::from_millis(550)).await; 
    let content_before_stop = fs::read_to_string(&output_file_path).unwrap_or_default();
    let executions_before_stop = content_before_stop.lines().count();
    assert!(executions_before_stop >= 3, "Periodic A should run >= 3 times. Found: {}", executions_before_stop);
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'B']).await?.status, TaskStatus::Completed, "Task B should complete");
    stop_task(&fixture.mission, fixture.task_ids[&'A']).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let content_after_stop = fs::read_to_string(&output_file_path).unwrap_or_default();
    assert_eq!(content_after_stop.lines().count(), executions_before_stop, "Periodic A should stop. Before: {}, After: {}", executions_before_stop, content_after_stop.lines().count());
    let a_info = fixture.mission.get_task_info(fixture.task_ids[&'A']).await?;
    assert_eq!(a_info.status, TaskStatus::Pending, "Periodic A should be Pending after stop.");
    Ok(())
}

#[tokio::test]
async fn test_periodic_task_error_handling() -> Result<()> {
    let (mut config, task_ids) = create_periodic_task_config();
    if let Some(task_a_config) = config.tasks.iter_mut().find(|t| t.id == task_ids[&'A']) {
        task_a_config.command = "exit 1".to_string(); 
    }
    if let Some(task_b_config) = config.tasks.iter_mut().find(|t| t.id == task_ids[&'B']) {
        task_b_config.dependencies = vec![]; 
    }
    let fixture = MissionFixture::new(config, task_ids.clone()).await?;
    let mission_clone = fixture.mission.clone();
    tokio::spawn(async move { run(&mission_clone).await.unwrap_or_else(|e| log::error!("Mission run failed: {:?}", e)); });
    tokio::time::sleep(Duration::from_millis(550)).await; 
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'B']).await?.status, TaskStatus::Completed, "Task B should complete.");
    let a_status = fixture.mission.get_task_info(fixture.task_ids[&'A']).await?.status;
    assert_ne!(a_status, TaskStatus::Completed, "Failing periodic A shouldn't be Completed. Status: {:?}", a_status);
    assert_eq!(a_status, TaskStatus::Running, "Failing periodic A should be Running/Pending. Status: {:?}", a_status); 
    stop_task(&fixture.mission, fixture.task_ids[&'A']).await?;
    Ok(())
}

#[tokio::test]
async fn test_task_info_retrieval() -> Result<()> {
    let fixture = MissionFixture::new_standard().await?;
    fixture.run_and_wait_for_all(Duration::from_secs(10)).await?;
    let all_info = get_all_task_info(&fixture.mission).await;
    assert_eq!(all_info.len(), 5, "Should have info for all 5 tasks");
    for (_, info) in all_info {
        assert_eq!(info.status, TaskStatus::Completed, "All tasks should be completed");
        assert!(info.start_time.is_some() && info.end_time.is_some() && info.execution_order.is_some());
    }
    Ok(())
}

// --- New tests for stop_task ---

#[tokio::test]
async fn test_stop_task_with_chained_dependencies() -> Result<()> {
    let (config, task_ids) = create_chained_dependency_config(); 
    let fixture = MissionFixture::new(config, task_ids.clone()).await?;
    let mission_clone = fixture.mission.clone();
    let run_handle = tokio::spawn(async move { run(&mission_clone).await });
    tokio::time::sleep(Duration::from_millis(20)).await; 
    let task_a_id = fixture.task_ids[&'A'];
    stop_task(&fixture.mission, task_a_id).await?;
    tokio::time::sleep(Duration::from_millis(100)).await; 
    assert_eq!(get_task_info(&fixture.mission, task_a_id).await?.status, TaskStatus::Pending, "Task A should be Pending after stop");
    let task_b_id = fixture.task_ids[&'B'];
    assert_eq!(get_task_info(&fixture.mission, task_b_id).await?.status, TaskStatus::Pending, "Task B should be Pending due to A being stopped");
    let task_c_id = fixture.task_ids[&'C'];
    assert_eq!(get_task_info(&fixture.mission, task_c_id).await?.status, TaskStatus::Pending, "Task C should be Pending");
    let task_d_id = fixture.task_ids[&'D'];
    assert_eq!(get_task_info(&fixture.mission, task_d_id).await?.status, TaskStatus::Pending, "Task D should be Pending");
    if let Err(e) = tokio::time::timeout(Duration::from_secs(5), run_handle).await {
         log::warn!("Mission run timed out or panicked after stop: {:?}", e);
    }
    assert_eq!(get_task_info(&fixture.mission, task_a_id).await?.status, TaskStatus::Pending, "Task A should remain Pending");
    assert_eq!(get_task_info(&fixture.mission, task_b_id).await?.status, TaskStatus::Pending, "Task B should remain Pending");
    assert_eq!(get_task_info(&fixture.mission, task_c_id).await?.status, TaskStatus::Pending, "Task C should remain Pending");
    assert_eq!(get_task_info(&fixture.mission, task_d_id).await?.status, TaskStatus::Pending, "Task D should remain Pending");
    Ok(())
}

#[tokio::test]
async fn test_stop_task_with_fan_out_dependencies() -> Result<()> {
    let (config, task_ids) = create_fan_out_dependency_config(); 
    let fixture = MissionFixture::new(config, task_ids.clone()).await?;
    let mission_clone = fixture.mission.clone();
    tokio::spawn(async move { run(&mission_clone).await.unwrap_or_else(|e| log::warn!("Mission run failed: {:?}", e)); });
    tokio::time::sleep(Duration::from_millis(50)).await;
    stop_task(&fixture.mission, fixture.task_ids[&'A']).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    for task_char in ['A', 'B', 'C', 'D'] {
        assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&task_char]).await?.status, TaskStatus::Pending, "Task {} should be Pending", task_char);
    }
    Ok(())
}

#[tokio::test]
async fn test_stop_task_with_fan_in_dependencies() -> Result<()> {
    let (config, task_ids) = create_fan_in_dependency_config(); 
    let fixture = MissionFixture::new(config, task_ids.clone()).await?;
    let mission_clone_part1 = fixture.mission.clone();
    tokio::spawn(async move { run(&mission_clone_part1).await.unwrap_or_else(|e| log::warn!("Mission run failed: {:?}", e)); });
    tokio::time::sleep(Duration::from_millis(50)).await; // Allow tasks to be scheduled
    stop_task(&fixture.mission, fixture.task_ids[&'A']).await?; // Stop A. B should now be able to start.
    // Task B command is "sleep 0.5; echo Task B". Wait for it to complete.
    tokio::time::sleep(Duration::from_millis(700)).await; // Increased from 200ms to allow B (0.5s) to complete + buffer
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'A']).await?.status, TaskStatus::Pending, "Part 1: Task A Pending");
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'B']).await?.status, TaskStatus::Completed, "Part 1: Task B Completed");
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'C']).await?.status, TaskStatus::Pending, "Part 1: Task C Pending (A stopped)");
    stop_task(&fixture.mission, fixture.task_ids[&'B']).await?; 
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'B']).await?.status, TaskStatus::Pending, "Part 2: Task B Pending after stop");
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'C']).await?.status, TaskStatus::Pending, "Part 2: Task C still Pending");
    Ok(())
}

#[tokio::test]
async fn test_stop_periodic_task_preventing_dependent_normal_task() -> Result<()> {
    let (config, task_ids) = create_periodic_a_normal_b_config(); 
    let fixture = MissionFixture::new(config, task_ids.clone()).await?;
    let mission_clone = fixture.mission.clone();
    tokio::spawn(async move { run(&mission_clone).await.unwrap_or_else(|e| log::warn!("Mission run failed: {:?}", e)); });
    tokio::time::sleep(Duration::from_millis(250)).await; 
    stop_task(&fixture.mission, fixture.task_ids[&'A']).await?;
    tokio::time::sleep(Duration::from_millis(150)).await; 
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'A']).await?.status, TaskStatus::Pending, "Periodic A should be Pending");
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'B']).await?.status, TaskStatus::Pending, "Normal B should be Pending");
    Ok(())
}

#[tokio::test]
async fn test_stop_normal_task_preventing_dependent_periodic_task() -> Result<()> {
    let (mut config, task_ids) = create_normal_a_periodic_b_config(); 
    let temp_dir_task_b = tempfile::tempdir()?;
    let output_file_b = temp_dir_task_b.path().join("periodic_b_output.txt");
    if let Some(task_b_config) = config.tasks.iter_mut().find(|t| t.id == task_ids[&'B']) {
        task_b_config.command = format!("echo 'Periodic B running' >> {}", output_file_b.display());
    }
    let fixture = MissionFixture::new(config, task_ids.clone()).await?;
    let mission_clone = fixture.mission.clone();
    tokio::spawn(async move { run(&mission_clone).await.unwrap_or_else(|e| log::warn!("Mission run failed: {:?}", e)); });
    tokio::time::sleep(Duration::from_millis(200)).await; 
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'A']).await?.status, TaskStatus::Completed, "Normal A should complete");
    tokio::time::sleep(Duration::from_millis(350)).await; 
    let executions_b_before_stop = fs::read_to_string(&output_file_b).unwrap_or_default().lines().count();
    assert!(executions_b_before_stop >= 2, "Periodic B should run >= 2 times. Found: {}", executions_b_before_stop);
    stop_task(&fixture.mission, fixture.task_ids[&'A']).await?; 
    tokio::time::sleep(Duration::from_millis(250)).await; 
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'A']).await?.status, TaskStatus::Pending, "Normal A should be Pending after stop");
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'B']).await?.status, TaskStatus::Pending, "Periodic B should be Pending");
    let executions_b_after_stop = fs::read_to_string(&output_file_b).unwrap_or_default().lines().count();
    assert_eq!(executions_b_after_stop, executions_b_before_stop, "Periodic B should stop. Before: {}, After: {}", executions_b_before_stop, executions_b_after_stop);
    Ok(())
}

#[tokio::test]
async fn test_stop_periodic_task_preventing_dependent_periodic_task() -> Result<()> {
    let temp_dir = TempDir::new()?; 
    let task_a_output_path = temp_dir.path().join("task_a_output.txt");
    let task_b_output_path = temp_dir.path().join("task_b_output.txt");
    let task_b_signal_path = temp_dir.path().join("task_b_signal.txt");

    // Ensure paths are strings for the config helper
    let task_a_output_str = task_a_output_path.to_str().unwrap().to_string();
    let task_b_output_str = task_b_output_path.to_str().unwrap().to_string();
    let task_b_signal_str = task_b_signal_path.to_str().unwrap().to_string();

    let (config, task_ids) = create_periodic_a_periodic_b_config(
        task_a_output_str.clone(),
        task_b_output_str.clone(),
        task_b_signal_str.clone(),
    );

    let fixture = MissionFixture::new(config, task_ids.clone()).await?;
    let mission_clone = fixture.mission.clone();

    let mission_run_handle = tokio::spawn(async move {
        if let Err(e) = run(&mission_clone).await {
            log::error!("Mission run failed: {:?}", e);
        }
    });

    // Wait for Task B to signal its first run
    let wait_for_signal_timeout = Duration::from_secs(10); 
    let poll_interval = Duration::from_millis(50); 
    let mut signal_detected = false;
    let start_wait_for_signal = Instant::now();

    log::info!("Waiting for Task B signal file: {}", task_b_signal_path.display());
    while start_wait_for_signal.elapsed() < wait_for_signal_timeout {
        if task_b_signal_path.exists() {
            if fs::read_to_string(&task_b_signal_path)?.contains("Signal created by Task B") {
                 signal_detected = true;
                 log::info!("Task B signal file detected and verified at {:?}.", start_wait_for_signal.elapsed());
                 break;
            }
        }
        tokio::time::sleep(poll_interval).await;
    }

    if !signal_detected {
        let task_a_status_info = get_task_info(&fixture.mission, fixture.task_ids[&'A']).await;
        let task_b_status_info = get_task_info(&fixture.mission, fixture.task_ids[&'B']).await;
        let task_a_log_content = fs::read_to_string(&task_a_output_path).unwrap_or_default();
        let task_b_log_content = fs::read_to_string(&task_b_output_path).unwrap_or_default();
        mission_run_handle.abort();
        panic!(
            "Task B did not signal its first run within timeout ({:?}).\n\
            Task A status: {:?}, Task B status: {:?}\n\
            Task A log ({} lines): {}\n\
            Task B log ({} lines): {}",
            wait_for_signal_timeout,
            task_a_status_info.map(|i|i.status).unwrap_or(TaskStatus::Pending), // Use Pending as default for panic msg
            task_b_status_info.map(|i|i.status).unwrap_or(TaskStatus::Pending), // Use Pending as default for panic msg
            task_a_log_content.lines().count(), task_a_log_content,
            task_b_log_content.lines().count(), task_b_log_content
        );
    }
    
    let executions_b_at_signal = fs::read_to_string(&task_b_output_path)
        .unwrap_or_default()
        .lines()
        .count();
    assert!(executions_b_at_signal >= 1, "Task B should have at least 1 execution entry by the time signal is detected. Found: {}", executions_b_at_signal);

    log::info!("Stopping Task A after Task B signaled execution (Task B executed {} times).", executions_b_at_signal);
    stop_task(&fixture.mission, fixture.task_ids[&'A']).await?;

    // Wait a bit to ensure Task B stops due to Task A stopping.
    // B's interval is 100ms. Wait for a few of its potential cycles + propagation time.
    tokio::time::sleep(Duration::from_millis(500)).await; 

    assert_eq!(
        get_task_info(&fixture.mission, fixture.task_ids[&'A']).await?.status,
        TaskStatus::Pending,
        "Periodic Task A should be Pending after stop"
    );
    assert_eq!(
        get_task_info(&fixture.mission, fixture.task_ids[&'B']).await?.status,
        TaskStatus::Pending,
        "Periodic Task B should be Pending as its dependency A was stopped"
    );
    
    let final_task_b_executions = fs::read_to_string(&task_b_output_path)
        .unwrap_or_default()
        .lines()
        .count();
    
    // Task B ran to create the signal. After A is stopped, B should not execute significantly more.
    // Allow for a small grace window (e.g., up to 2 additional runs) if B was mid-cycle or if event processing had slight delays.
    assert!(
        final_task_b_executions <= executions_b_at_signal + 2, 
        "Task B should not execute much further after Task A is stopped. Executions at signal: {}, Final executions: {}",
        executions_b_at_signal, final_task_b_executions
    );
    
    let task_a_executions_total = fs::read_to_string(&task_a_output_path)
        .unwrap_or_default()
        .lines()
        .count();
    assert!(task_a_executions_total > 0, "Task A should have run at least once.");


    mission_run_handle.abort(); 
    Ok(())
}

#[tokio::test]
async fn test_stop_completed_task_and_dependent_tasks() -> Result<()> {
    let (config, task_ids) = create_simple_a_b_config();
    let fixture = MissionFixture::new(config, task_ids.clone()).await?;
    fixture.run_and_wait_for_all(Duration::from_secs(5)).await?;
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'A']).await?.status, TaskStatus::Completed, "A completed");
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'B']).await?.status, TaskStatus::Completed, "B completed");
    stop_task(&fixture.mission, fixture.task_ids[&'A']).await?; 
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'A']).await?.status, TaskStatus::Pending, "A Pending after stop");
    assert_eq!(fixture.mission.get_task_info(fixture.task_ids[&'B']).await?.status, TaskStatus::Pending, "B Pending after A stopped");
    Ok(())
}

#[tokio::test]
async fn test_stop_non_existent_task() -> Result<()> {
    let fixture = MissionFixture::new_standard().await?;
    let non_existent_task_id = Uuid::new_v4();
    let result = stop_task(&fixture.mission, non_existent_task_id).await;
    assert!(result.is_err(), "Stopping non-existent task should err.");
    if let Err(NightError::Mission(msg)) = result {
        assert!(msg.contains("not found"), "Error message should indicate task not found.");
    } else {
        panic!("Expected NightError::Mission, got {:?}", result);
    }
    Ok(())
}
