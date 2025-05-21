use crate::common::types::MissionConfig;
use crate::utils::error::{NightError, Result};
use serde_json;
use serde_yaml;
use std::fs;
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

        Self::validate_config(&config)?;

        Ok(config)
    }

    fn validate_config(config: &MissionConfig) -> Result<()> {
        // Check for unique task IDs
        let mut task_ids = std::collections::HashSet::new();
        for task in &config.tasks {
            if !task_ids.insert(task.id) {
                return Err(NightError::Config(format!(
                    "Duplicate task ID: {}",
                    task.id
                )));
            }
        }

        // Validate task dependencies
        for task in &config.tasks {
            for dep_id in &task.dependencies {
                if !task_ids.contains(dep_id) {
                    return Err(NightError::Config(format!(
                        "Task {} depends on non-existent task {}",
                        task.id, dep_id
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
        assert_eq!(config.tasks[1].dependencies[0], uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap());
        
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
}
