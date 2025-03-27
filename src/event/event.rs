//! Event definitions for the Night event system.
//!
//! This module defines the event types and event structure used in the
//! event-based communication system.

use crate::common::types::TaskStatus;
use uuid::Uuid;

/// Types of events that can be emitted in the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    /// Task status has changed
    TaskStatusChanged,
    /// Task has completed execution
    TaskCompleted,
    /// Task has failed execution
    TaskFailed,
    /// Mission has started
    MissionStarted,
    /// Mission has completed
    MissionCompleted,
    /// Mission has failed
    MissionFailed,
}

/// Event structure containing event data.
#[derive(Debug, Clone)]
pub struct Event {
    /// Type of the event
    pub event_type: EventType,
    /// ID of the task that generated the event (if applicable)
    pub task_id: Option<Uuid>,
    /// Previous status of the task (if applicable)
    pub previous_status: Option<TaskStatus>,
    /// New status of the task (if applicable)
    pub new_status: Option<TaskStatus>,
    /// Additional data associated with the event
    pub data: Option<String>,
    /// Timestamp when the event was created
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Event {
    /// Create a new event
    pub fn new(
        event_type: EventType,
        task_id: Option<Uuid>,
        previous_status: Option<TaskStatus>,
        new_status: Option<TaskStatus>,
        data: Option<String>,
    ) -> Self {
        Event {
            event_type,
            task_id,
            previous_status,
            new_status,
            data,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create a task status changed event
    pub fn task_status_changed(
        task_id: Uuid,
        previous_status: TaskStatus,
        new_status: TaskStatus,
    ) -> Self {
        Event::new(
            EventType::TaskStatusChanged,
            Some(task_id),
            Some(previous_status),
            Some(new_status),
            None,
        )
    }

    /// Create a task completed event
    pub fn task_completed(task_id: Uuid) -> Self {
        Event::new(
            EventType::TaskCompleted,
            Some(task_id),
            Some(TaskStatus::Running),
            Some(TaskStatus::Completed),
            None,
        )
    }

    /// Create a task failed event
    pub fn task_failed(task_id: Uuid, error_message: String) -> Self {
        Event::new(
            EventType::TaskFailed,
            Some(task_id),
            Some(TaskStatus::Running),
            Some(TaskStatus::Failed),
            Some(error_message),
        )
    }

    /// Create a mission started event
    pub fn mission_started() -> Self {
        Event::new(EventType::MissionStarted, None, None, None, None)
    }

    /// Create a mission completed event
    pub fn mission_completed() -> Self {
        Event::new(EventType::MissionCompleted, None, None, None, None)
    }

    /// Create a mission failed event
    pub fn mission_failed(error_message: String) -> Self {
        Event::new(
            EventType::MissionFailed,
            None,
            None,
            None,
            Some(error_message),
        )
    }
}