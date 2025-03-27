pub mod common;
pub mod config;
pub mod event;
pub mod mission;
pub mod task;
pub mod utils;

pub use config::config::ConfigManager;
pub use mission::mission::Mission;
pub use utils::error::{NightError, Result};

/// Initialize the Night system with a configuration file.
pub async fn init(config_path: &str) -> Result<Mission> {
    let config = ConfigManager::load_config(config_path)?;
    Mission::new(config).await
}

/// Start the mission execution.
pub async fn run(mission: &Mission) -> Result<()> {
    mission.start().await
}

/// Stop a specific task in the mission.
pub async fn stop_task(mission: &Mission, task_id: uuid::Uuid) -> Result<()> {
    mission.stop_task(task_id).await
}

/// Get information about all tasks in the mission.
pub async fn get_all_task_info(
    mission: &Mission,
) -> std::collections::HashMap<uuid::Uuid, common::types::TaskInfo> {
    mission.get_all_task_info().await
}

/// Get information about a specific task in the mission.
pub async fn get_task_info(
    mission: &Mission,
    task_id: uuid::Uuid,
) -> Result<common::types::TaskInfo> {
    mission.get_task_info(task_id).await
}
