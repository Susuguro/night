use night::mission::topology::TopologyManager;
use night::common::types::TaskConfig;
use uuid::Uuid;
use std::collections::HashSet; // Added for comparing unsorted lists

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_task(id: u128, deps: Vec<u128>) -> TaskConfig {
        TaskConfig {
            name: format!("Task {}", id),
            id: Uuid::from_u128(id),
            command: "echo test".to_string(),
            is_periodic: false,
            interval: "0".to_string(),
            importance: 1,
            dependencies: deps.into_iter().map(Uuid::from_u128).collect(),
        }
    }

    #[test]
    fn test_valid_topology() {
        let tasks = vec![
            create_test_task(1, vec![]),
            create_test_task(2, vec![1]),
            create_test_task(3, vec![1]),
            create_test_task(4, vec![2, 3]),
        ];

        let topology = TopologyManager::new(tasks).unwrap();
        let order = topology.get_execution_order();
        assert_eq!(order.len(), 3);
        assert_eq!(order[0], vec![Uuid::from_u128(1)]);
        // For level 1, order doesn't matter, so check presence
        let level1_tasks: HashSet<Uuid> = order[1].iter().cloned().collect();
        let expected_level1: HashSet<Uuid> = [Uuid::from_u128(2), Uuid::from_u128(3)].iter().cloned().collect();
        assert_eq!(level1_tasks, expected_level1);
        assert_eq!(order[2], vec![Uuid::from_u128(4)]);
    }

    #[test]
    fn test_cyclic_topology() {
        let tasks = vec![
            create_test_task(1, vec![3]),
            create_test_task(2, vec![1]),
            create_test_task(3, vec![2]),
        ];

        let result = TopologyManager::new(tasks);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_dependency() {
        let tasks = vec![
            create_test_task(1, vec![]),
            create_test_task(2, vec![3]), // Task 3 doesn't exist
        ];

        let result = TopologyManager::new(tasks);
        assert!(result.is_err());
    }

    #[test]
    fn test_deeply_nested_topology() {
        let tasks = vec![
            create_test_task(1, vec![]),
            create_test_task(2, vec![1]),
            create_test_task(3, vec![2]),
            create_test_task(4, vec![3]),
        ];
        let topology = TopologyManager::new(tasks).unwrap();
        let order = topology.get_execution_order();
        assert_eq!(order.len(), 4);
        assert_eq!(order[0], vec![Uuid::from_u128(1)]);
        assert_eq!(order[1], vec![Uuid::from_u128(2)]);
        assert_eq!(order[2], vec![Uuid::from_u128(3)]);
        assert_eq!(order[3], vec![Uuid::from_u128(4)]);
    }

    #[test]
    fn test_multiple_independent_paths() {
        let tasks = vec![
            create_test_task(1, vec![]),
            create_test_task(2, vec![1]),
            create_test_task(3, vec![]),
            create_test_task(4, vec![3]),
        ];
        let topology = TopologyManager::new(tasks).unwrap();
        let order = topology.get_execution_order();
        assert_eq!(order.len(), 2);

        let level0_tasks: HashSet<Uuid> = order[0].iter().cloned().collect();
        let expected_level0: HashSet<Uuid> = [Uuid::from_u128(1), Uuid::from_u128(3)].iter().cloned().collect();
        assert_eq!(level0_tasks, expected_level0);

        let level1_tasks: HashSet<Uuid> = order[1].iter().cloned().collect();
        let expected_level1: HashSet<Uuid> = [Uuid::from_u128(2), Uuid::from_u128(4)].iter().cloned().collect();
        assert_eq!(level1_tasks, expected_level1);
    }

    #[test]
    fn test_get_task_and_dependencies() {
        let task1_id = Uuid::from_u128(1);
        let task2_id = Uuid::from_u128(2);
        let task3_id = Uuid::from_u128(3); // Non-existent for some tests
        let task1_config = create_test_task(1, vec![]);
        let task2_config = create_test_task(2, vec![1]);

        let tasks = vec![task1_config.clone(), task2_config.clone()];
        let topology = TopologyManager::new(tasks).unwrap();

        // Test get_task for existing task
        let retrieved_task1 = topology.get_task(&task1_id);
        assert!(retrieved_task1.is_some());
        assert_eq!(retrieved_task1.unwrap().id, task1_id);
        assert_eq!(retrieved_task1.unwrap().name, "Task 1");

        // Test get_task for non-existent task
        assert!(topology.get_task(&task3_id).is_none());

        // Test get_dependencies for a task with dependencies
        let deps_task2 = topology.get_dependencies(&task2_id);
        assert!(deps_task2.is_some());
        assert_eq!(deps_task2.unwrap(), &vec![task1_id]);

        // Test get_dependencies for a task with no dependencies
        let deps_task1 = topology.get_dependencies(&task1_id);
        assert!(deps_task1.is_some());
        assert!(deps_task1.unwrap().is_empty());

        // Test get_dependencies for a non-existent task
        assert!(topology.get_dependencies(&task3_id).is_none());
    }
}
