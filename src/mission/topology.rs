use crate::common::types::TaskConfig;
use crate::utils::error::{NightError, Result};
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

pub struct TopologyManager {
    tasks: HashMap<Uuid, TaskConfig>,
    dependencies: HashMap<Uuid, Vec<Uuid>>,
    reverse_dependencies: HashMap<Uuid, Vec<Uuid>>,
}

impl TopologyManager {
    pub fn new(tasks: Vec<TaskConfig>) -> Result<Self> {
        let mut topology = TopologyManager {
            tasks: HashMap::new(),
            dependencies: HashMap::new(),
            reverse_dependencies: HashMap::new(),
        };

        for task in tasks {
            topology.add_task(task)?;
        }

        topology.validate()?;

        Ok(topology)
    }

    fn add_task(&mut self, task: TaskConfig) -> Result<()> {
        let task_id = task.id;
        self.tasks.insert(task_id, task.clone());
        self.dependencies.insert(task_id, task.dependencies.clone());

        for dep_id in &task.dependencies {
            self.reverse_dependencies
                .entry(*dep_id)
                .or_insert_with(Vec::new)
                .push(task_id);
        }

        Ok(())
    }

    fn validate(&self) -> Result<()> {
        // Check for cycles
        if let Err(cycle) = self.detect_cycle() {
            return Err(NightError::Mission(format!(
                "Cyclic dependency detected: {:?}",
                cycle
            )));
        }

        // Check for missing dependencies
        for (task_id, deps) in &self.dependencies {
            for dep_id in deps {
                if !self.tasks.contains_key(dep_id) {
                    return Err(NightError::Mission(format!(
                        "Task {} depends on non-existent task {}",
                        task_id, dep_id
                    )));
                }
            }
        }

        Ok(())
    }

    fn detect_cycle(&self) -> Result<()> {
        let mut visited = HashSet::new();
        let mut stack = HashSet::new();

        for &task_id in self.tasks.keys() {
            if self.is_cyclic(task_id, &mut visited, &mut stack) {
                return Err(NightError::Mission(format!(
                    "Cyclic dependency detected: {:?}",
                    stack
                )));
            }
        }

        Ok(())
    }

    fn is_cyclic(
        &self,
        task_id: Uuid,
        visited: &mut HashSet<Uuid>,
        stack: &mut HashSet<Uuid>,
    ) -> bool {
        if stack.contains(&task_id) {
            return true;
        }

        if visited.contains(&task_id) {
            return false;
        }

        visited.insert(task_id);
        stack.insert(task_id);

        if let Some(deps) = self.dependencies.get(&task_id) {
            for &dep_id in deps {
                if self.is_cyclic(dep_id, visited, stack) {
                    return true;
                }
            }
        }

        stack.remove(&task_id);
        false
    }

    /// Calculates the execution order of tasks based on their dependencies.
    /// This method implements Kahn's algorithm for topological sorting.
    /// Returns a vector of vectors, where each inner vector represents a level
    /// of tasks that can be executed in parallel.
    pub fn get_execution_order(&self) -> Vec<Vec<Uuid>> {
        // Initialize a map to store the in-degree of each task.
        // The in-degree of a task is the number of tasks that depend on it.
        let mut in_degree = HashMap::new();
        for &task_id in self.tasks.keys() {
            in_degree.insert(
                task_id,
                // Get the number of dependencies for the current task.
                // If the task has no entry in `self.dependencies` or its dependency list is empty,
                // its in-degree is 0.
                self.dependencies.get(&task_id).map_or(0, |deps| deps.len()),
            );
        }

        // Initialize a queue with all tasks that have an in-degree of 0.
        // These are the tasks that have no prerequisites and can be executed first.
        let mut queue = VecDeque::new();
        for (&task_id, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(task_id);
            }
        }

        // This will store the final result, where each inner vector is a "level" of execution.
        let mut result = Vec::new();

        // Process tasks level by level until the queue is empty.
        while !queue.is_empty() {
            // `level` will store all tasks that can be executed in the current parallel batch.
            let mut level = Vec::new();
            // Iterate over all tasks currently in the queue (tasks with in-degree 0 at this stage).
            // The number of iterations is fixed to the current queue size to process one full level.
            for _ in 0..queue.len() {
                // Dequeue a task. This task is now considered "executed".
                let task_id = queue.pop_front().unwrap(); // Safe due to loop condition
                level.push(task_id);

                // For the "executed" task, find all tasks that depend on it (reverse dependencies).
                if let Some(rev_deps) = self.reverse_dependencies.get(&task_id) {
                    for &dependent_task_id in rev_deps {
                        // Decrement the in-degree of each dependent task.
                        let entry = in_degree.get_mut(&dependent_task_id).unwrap(); // Dependent must exist
                        *entry -= 1;
                        // If a dependent task's in-degree becomes 0, it means all its prerequisites
                        // have been met, so it can be added to the queue for the next level of execution.
                        if *entry == 0 {
                            queue.push_back(dependent_task_id);
                        }
                    }
                }
            }
            // Add the completed level (all tasks that could run in parallel) to the result.
            result.push(level);
        }

        // If the result doesn't contain all tasks (e.g. sum of lengths of inner vecs != self.tasks.len()),
        // it implies a cycle was present. However, `validate()` should have caught this.
        // This function assumes a valid, cycle-free topology.
        result
    }

    pub fn get_task(&self, task_id: &Uuid) -> Option<&TaskConfig> {
        self.tasks.get(task_id)
    }

    pub fn get_dependencies(&self, task_id: &Uuid) -> Option<&Vec<Uuid>> {
        self.dependencies.get(task_id)
    }
}
