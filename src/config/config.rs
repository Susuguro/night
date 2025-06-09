use crate::common::types::MissionConfig;
use crate::utils::error::{NightError, Result};
use serde_json;
use serde_yaml;
use std::fs;
use uuid::Uuid;
use std::path::Path;

pub struct ConfigManager;

impl ConfigManager {
    pub fn load_config<P: AsRef<Path>>(path: P) -> Result<MissionConfig> {
        let path_ref = path.as_ref();
        let extension = path_ref
            .extension()
            .and_then(|ext| ext.to_str())
            .ok_or_else(|| NightError::Config("File has no extension".to_string()))?;

        let config_str = fs::read_to_string(path_ref)
            .map_err(|e| NightError::Config(format!("Failed to read config file: {}", e)))?;

        let config: MissionConfig = match extension {
            "json" => serde_json::from_str(&config_str)
                .map_err(|e| NightError::Config(format!("Failed to parse JSON config: {}", e)))?, // Or ideally NightError::JsonSerialization if that was distinct
            "yaml" | "yml" => serde_yaml::from_str(&config_str)?, // This will now use `From<serde_yaml::Error>` for NightError
            _ => {
                return Err(NightError::Config(format!(
                    "Unsupported file extension: {}",
                    extension
                )))
            }
        };

        let mut config = config; // Make config mutable

        for task_config in config.tasks.iter_mut() {
            if task_config.id.is_none() {
                task_config.id = Some(Uuid::new_v4());
            }
        }

        Self::validate_config(&config)?;

        Ok(config)
    }

    fn validate_config(config: &MissionConfig) -> Result<()> {
        // Check for unique task IDs
        let mut task_ids = std::collections::HashSet::new();
        for task in &config.tasks {
            if !task_ids.insert(task.id.expect("ID should be present at validation")) {
                return Err(NightError::Config(format!(
                    "Duplicate task ID: {}",
                    task.id.expect("ID should be present at validation")
                )));
            }
        }

        // Validate task dependencies
        for task in &config.tasks {
            for dep_id in &task.dependencies {
                if !task_ids.contains(dep_id) {
                    return Err(NightError::Config(format!(
                        "Task {} depends on non-existent task {}",
                        task.id.expect("ID should be present at validation"), dep_id
                    )));
                }
            }
        }

        // Additional validations can be added here

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_load_valid_config() {
        let config_json = r#"
        {
            "name": "Test Mission",
            "tasks": [
                {
                    "name": "Task 1",
                    "id": "00000000-0000-0000-0000-000000000001",
                    "command": "echo Hello",
                    "is_periodic": false,
                    "interval": "",
                    "importance": 1,
                    "dependencies": []
                }
            ]
        }
        "#;

        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        fs::write(&config_path, config_json).unwrap();

        let result = ConfigManager::load_config(config_path);
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.tasks[0].id, Some(Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()));
    }

    #[test]
    fn test_load_invalid_config() {
        let config_json = r#"
        {
            "name": "Invalid Mission",
            "tasks": [
                {
                    "name": "Task 1",
                    "id": "00000000-0000-0000-0000-000000000001",
                    "command": "echo Hello",
                    "is_periodic": false,
                    "interval": "",
                    "importance": 1,
                    "dependencies": ["00000000-0000-0000-0000-000000000002"]
                }
            ]
        }
        "#;

        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        fs::write(&config_path, config_json).unwrap();

        let result = ConfigManager::load_config(config_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_valid_yaml_config() {
        let config_yaml = r#"
name: Test Mission YAML
tasks:
  - name: Task 1 YAML
    id: "00000000-0000-0000-0000-000000000001"
    command: echo Hello YAML
    is_periodic: false
    interval: ""
    importance: 1
    dependencies: []
  - name: Task 2 YAML
    id: "00000000-0000-0000-0000-000000000002"
    command: echo Hello again YAML
    is_periodic: false
    interval: ""
    importance: 1
    dependencies:
      - "00000000-0000-0000-0000-000000000001"
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_yaml.as_bytes()).unwrap();
        let (file, path) = temp_file.keep().unwrap(); // Keep the file for the duration of the test
        // Manually create a new PathBuf for the YAML file with the correct extension
        let yaml_path = path.with_extension("yaml");
        fs::rename(&path, &yaml_path).unwrap();


        let result = ConfigManager::load_config(&yaml_path);
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.name, "Test Mission YAML");
        assert_eq!(config.tasks.len(), 2);
        assert_eq!(config.tasks[0].name, "Task 1 YAML");
        assert_eq!(config.tasks[0].id, Some(Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()));
        assert_eq!(config.tasks[1].id, Some(Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap()));
        assert_eq!(config.tasks[1].dependencies[0], Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap());
        
        // Clean up the temporary file
        fs::remove_file(yaml_path).unwrap();
        drop(file);
    }

    #[test]
    fn test_load_yaml_config_invalid_structure() {
        let invalid_structure_yaml = r#"
name: Invalid Structure YAML
tasks:
  - name: Task 1 YAML
    id: "00000000-0000-0000-0000-000000000001"
    command: echo Hello YAML
    is_periodic: false
    interval: ""
    importance: 1
    dependencies: ["00000000-0000-0000-0000-000000000002"] # Non-existent dependency
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(invalid_structure_yaml.as_bytes()).unwrap();
        let (file, path) = temp_file.keep().unwrap();
        let yaml_path = path.with_extension("yaml");
        fs::rename(&path, &yaml_path).unwrap();

        let result = ConfigManager::load_config(&yaml_path);
        assert!(result.is_err());

        // Ensure the error is due to validation
        if let Err(NightError::Config(msg)) = result {
            assert!(msg.contains("depends on non-existent task"));
        } else {
            panic!("Expected a Config error indicating a validation failure");
        }

        fs::remove_file(yaml_path).unwrap();
        drop(file);
    }

    #[test]
    fn test_load_invalid_yaml_config_syntax() {
        let invalid_yaml = r#"
[invalid key]: value 
tasks:
  - name: Task 1 YAML
    id: "00000000-0000-0000-0000-000000000001"
    command: echo Hello YAML
    is_periodic: false
    interval: ""
    importance: 1
    dependencies: []
  - name: Task 2 YAML
    id: "00000000-0000-0000-0000-000000000002"
    command: echo Hello again YAML
    is_periodic: false
    interval: ""
    importance: 1
    dependencies:
      - "00000000-0000-0000-0000-000000000001"
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(invalid_yaml.as_bytes()).unwrap();
        let (file, path) = temp_file.keep().unwrap();
        let yaml_path = path.with_extension("yaml");
        fs::rename(&path, &yaml_path).unwrap();

        let result = ConfigManager::load_config(&yaml_path);
        assert!(result.is_err());
        
        // Ensure the error is due to parsing and is the correct YamlSerialization variant
        match result {
            Err(NightError::YamlSerialization(_)) => {
                // This is the expected outcome.
            }
            Err(e) => {
                panic!("Expected NightError::YamlSerialization, but got a different error: {:?}", e);
            }
            Ok(_) => {
                panic!("Expected an error, but got Ok");
            }
        }

        fs::remove_file(yaml_path).unwrap();
        drop(file);
    }

    #[test]
    fn test_load_unsupported_extension() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"content").unwrap();
        let (file, path) = temp_file.keep().unwrap();
        let txt_path = path.with_extension("txt");
        fs::rename(&path, &txt_path).unwrap();

        let result = ConfigManager::load_config(&txt_path);
        assert!(result.is_err());

        if let Err(NightError::Config(msg)) = result {
            assert!(msg.contains("Unsupported file extension: txt"));
        } else {
            panic!("Expected a Config error indicating an unsupported file extension");
        }

        fs::remove_file(txt_path).unwrap();
        drop(file);
    }

    #[test]
    fn test_load_config_with_missing_ids_json() {
        let config_json = r#"
        {
            "name": "Test Mission JSON Missing IDs",
            "tasks": [
                {
                    "name": "Task 1 With ID",
                    "id": "11111111-1111-1111-1111-111111111111",
                    "command": "echo Task1",
                    "is_periodic": false,
                    "interval": "",
                    "importance": 1,
                    "dependencies": []
                },
                {
                    "name": "Task 2 Missing ID",
                    "command": "echo Task2",
                    "is_periodic": false,
                    "interval": "",
                    "importance": 1,
                    "dependencies": []
                }
            ]
        }
        "#;

        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config_missing_ids.json");
        fs::write(&config_path, config_json).unwrap();

        let result = ConfigManager::load_config(config_path);
        assert!(result.is_ok(), "load_config should succeed");
        let config = result.unwrap();

        assert_eq!(config.tasks.len(), 2);
        assert_eq!(config.tasks[0].id, Some(Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()), "Task 1 ID should match specified");
        assert!(config.tasks[1].id.is_some(), "Task 2 ID should be generated");
    }

    #[test]
    fn test_load_config_with_missing_ids_yaml() {
        let config_yaml = r#"
name: Test Mission YAML Missing IDs
tasks:
  - name: Task 1 With ID YAML
    id: "22222222-2222-2222-2222-222222222222"
    command: echo Task1 YAML
    is_periodic: false
    interval: ""
    importance: 1
    dependencies: []
  - name: Task 2 Missing ID YAML
    command: echo Task2 YAML
    is_periodic: false
    interval: ""
    importance: 1
    dependencies: []
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_yaml.as_bytes()).unwrap();
        let (file, path) = temp_file.keep().unwrap();
        let yaml_path = path.with_extension("yaml");
        fs::rename(&path, &yaml_path).unwrap();

        let result = ConfigManager::load_config(&yaml_path);
        assert!(result.is_ok(), "load_config should succeed for YAML with missing IDs");
        let config = result.unwrap();

        assert_eq!(config.tasks.len(), 2);
        assert_eq!(config.tasks[0].id, Some(Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap()), "Task 1 YAML ID should match specified");
        assert!(config.tasks[1].id.is_some(), "Task 2 YAML ID should be generated");

        fs::remove_file(yaml_path).unwrap();
        drop(file);
    }

    #[test]
    fn test_dependency_on_auto_generated_id_fails_if_fixed_uuid_expected() {
        let config_json = r#"
        {
            "name": "Dependency Test Mission",
            "tasks": [
                {
                    "name": "Task A (ID will be auto-generated)",
                    "command": "echo TaskA",
                    "is_periodic": false,
                    "interval": "",
                    "importance": 1,
                    "dependencies": []
                },
                {
                    "name": "Task B (depends on a fixed ID)",
                    "id": "33333333-3333-3333-3333-333333333333",
                    "command": "echo TaskB",
                    "is_periodic": false,
                    "interval": "",
                    "importance": 1,
                    "dependencies": ["00000000-0000-0000-0000-000000000001"]
                }
            ]
        }
        "#;
        // Task B depends on a UUID that won't match Task A's auto-generated ID.

        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config_dep_fail.json");
        fs::write(&config_path, config_json).unwrap();

        let result = ConfigManager::load_config(config_path);
        assert!(result.is_err(), "Validation should fail due to missing dependency");
        if let Err(NightError::Config(msg)) = result {
            assert!(msg.contains("depends on non-existent task"), "Error message should indicate non-existent task dependency");
        } else {
            panic!("Expected a Config error, got {:?}", result);
        }
    }

    #[test]
    fn test_dependency_on_specified_id_succeeds_when_dependent_has_auto_id() {
        let fixed_id_str = "44444444-4444-4444-4444-444444444444";
        let config_json = format!(r#"
        {{
            "name": "Dependency Test Mission Success",
            "tasks": [
                {{
                    "name": "Task A (Specified ID)",
                    "id": "{}",
                    "command": "echo TaskA",
                    "is_periodic": false,
                    "interval": "",
                    "importance": 1,
                    "dependencies": []
                }},
                {{
                    "name": "Task B (Auto-generated ID, depends on Task A)",
                    "command": "echo TaskB",
                    "is_periodic": false,
                    "interval": "",
                    "importance": 1,
                    "dependencies": ["{}"]
                }}
            ]
        }}
        "#, fixed_id_str, fixed_id_str);

        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config_dep_success.json");
        fs::write(&config_path, config_json).unwrap();

        let result = ConfigManager::load_config(config_path);
        assert!(result.is_ok(), "Validation should succeed. Error: {:?}", result.err());
        let config = result.unwrap();
        assert!(config.tasks[1].id.is_some(), "Task B (dependent) ID should be generated");
        assert_ne!(config.tasks[1].id, Some(Uuid::parse_str(fixed_id_str).unwrap()), "Task B's auto-generated ID should not be Task A's ID");
        assert_eq!(config.tasks[0].id, Some(Uuid::parse_str(fixed_id_str).unwrap()), "Task A's ID should be the specified one");
    }
}
