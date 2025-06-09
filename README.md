# night

[![Rust 2021+](https://img.shields.io/badge/rust-2021%2B-blue.svg)](https://www.rust-lang.org/)
[![License: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://opensource.org/licenses/MIT)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)](https://github.com/your-username/night/actions)

A scalable Task Queue for executing asynchronous tasks in topological order.

`night` allows you to define a mission with multiple tasks, where tasks can have dependencies on each other. The system ensures that tasks are executed only after their dependencies have been successfully completed. It leverages `tokio` for efficient asynchronous task management.

## Command-Line Interface (CLI)

The `night` CLI provides a way to manage and interact with your task missions.

### Commands

*   **`run <config_file>`**
    *   Initializes and starts a mission based on the provided configuration file.
    *   The configuration file (e.g., `config.yaml` or `config.json`) defines the tasks, their dependencies, and execution parameters.
    *   Example: `night run config.yaml`

*   **`stop <config_file> <task_id>`**
    *   Stops a specific running task within a mission.
    *   `<config_file>`: The configuration file used to start the mission.
    *   `<task_id>`: The unique identifier of the task to stop.
    *   Example: `night stop config.yaml my_specific_task`

*   **`info <config_file> [task_id]`**
    *   Retrieves information about tasks in a mission.
    *   `<config_file>`: The configuration file used to start the mission.
    *   `[task_id]` (optional): If provided, displays information for the specified task. Otherwise, displays information for all tasks in the mission.
    *   Example (all tasks): `night info config.yaml`
    *   Example (specific task): `night info config.yaml another_task`

## Library Usage

You can also use `night` as a library in your Rust projects to embed its task queuing and execution capabilities.

### Main Functions

*   **`init(config_path: &str) -> Result<Mission>`**:
    *   Initializes a new mission from a configuration file path.
    *   Returns a `Mission` object that can be used to manage tasks.

*   **`Mission::run(&self)`**:
    *   Starts the execution of all tasks in the mission according to their dependencies.

*   **`Mission::stop_task(&self, task_id: &str) -> Result<()>`**:
    *   Stops a specific task by its ID.

*   **`Mission::get_all_task_info(&self) -> Result<Vec<TaskInfo>>`**:
    *   Retrieves status and information for all tasks in the mission.

*   **`Mission::get_task_info(&self, task_id: &str) -> Result<TaskInfo>`**:
    *   Retrieves status and information for a specific task by its ID.

### Example (Conceptual)

```rust
use night::{init, Mission};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the mission from a config file
    let mission = init("path/to/your/config.yaml").await?;

    // Run the mission (tasks will execute based on dependencies)
    mission.run().await;

    // Example: Get info for a specific task
    match mission.get_task_info("some_task_id").await {
        Ok(info) => println!("Task 'some_task_id' status: {:?}", info.status),
        Err(e) => eprintln!("Error getting task info: {}", e),
    }

    // Example: Stop a task
    if let Err(e) = mission.stop_task("another_task_id").await {
        eprintln!("Error stopping task: {}", e);
    }

    // Keep the main thread alive if tasks are running in background,
    // or manage mission lifecycle as needed.
    // For instance, you might await specific task completions if required.

    Ok(())
}
```

## Technology

*   **Rust**: Built with Rust for performance, safety, and concurrency.
*   **Tokio**: Utilizes the `tokio` runtime for asynchronous operations, enabling efficient handling of many tasks concurrently.

## Configuration

Task missions are defined in configuration files (YAML or JSON format). These files specify:
*   Each task's ID, name, command/action to execute.
*   Dependencies between tasks.
*   Scheduling parameters (e.g., run once, periodic execution).

Refer to the project documentation or examples for the detailed configuration schema.

## Building from Source

1.  Ensure you have Rust and Cargo installed (see [rustup.rs](https://rustup.rs/)).
2.  Clone the repository: `git clone https://github.com/your-username/night.git`
3.  Navigate to the project directory: `cd night`
4.  Build the project: `cargo build --release`
5.  The executable will be located at `target/release/night`.

## Contributing

Contributions are welcome! Please feel free to submit issues, fork the repository, and create pull requests.

## License

This project is licensed under either of
*   Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
*   MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
at your option.
