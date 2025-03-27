//! Event listener definitions for the Night event system.
//!
//! This module defines the event listener and callback types used in the
//! event-based communication system.

use crate::event::event::{Event, EventType};
use crate::utils::error::Result;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use uuid::Uuid;

/// Type alias for event listener callback functions.
pub type ListenerCallback = Arc<dyn Fn(Event) -> Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>> + Send + Sync>;

/// Event listener structure.
#[derive(Clone)]
pub struct EventListener {
    /// Unique ID of the listener
    pub id: Uuid,
    /// Type of event this listener is interested in
    pub event_type: EventType,
    /// Optional task ID filter (only receive events for this task)
    pub task_id_filter: Option<Uuid>,
    /// Callback function to execute when an event is received
    pub callback: ListenerCallback,
}

impl EventListener {
    /// Create a new event listener
    pub fn new(
        event_type: EventType,
        task_id_filter: Option<Uuid>,
        callback: ListenerCallback,
    ) -> Self {
        EventListener {
            id: Uuid::new_v4(),
            event_type,
            task_id_filter,
            callback,
        }
    }

    /// Check if this listener is interested in the given event
    pub fn is_interested_in(&self, event: &Event) -> bool {
        // Check if event type matches
        if self.event_type != event.event_type {
            return false;
        }

        // If task_id_filter is set, check if event's task_id matches
        if let Some(filter_id) = self.task_id_filter {
            if let Some(event_task_id) = event.task_id {
                return filter_id == event_task_id;
            }
            return false;
        }

        // No task_id filter, so we're interested in this event
        true
    }

    /// Handle an event by calling the callback function
    pub async fn handle_event(&self, event: Event) -> Result<()> {
        if self.is_interested_in(&event) {
            let future = (self.callback)(event);
            Pin::from(future).await?
        }
        Ok(())
    }
}