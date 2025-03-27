//! Event system manager for Night.
//!
//! This module provides a central event system manager that handles event
//! subscription, publishing, and dispatching.

use crate::event::event::{Event, EventType};
use crate::event::listener::EventListener;
use crate::utils::error::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Event system manager that handles event subscription and publishing.
#[derive(Clone)]
pub struct EventSystem {
    /// Map of event types to listeners
    listeners: Arc<RwLock<HashMap<EventType, Vec<EventListener>>>>,
}

impl EventSystem {
    /// Create a new event system
    pub fn new() -> Self {
        EventSystem {
            listeners: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Subscribe to an event type
    pub async fn subscribe(&self, listener: EventListener) -> Result<Uuid> {
        let mut listeners = self.listeners.write().await;
        let entry = listeners.entry(listener.event_type).or_insert_with(Vec::new);
        let listener_id = listener.id;
        entry.push(listener);
        Ok(listener_id)
    }

    /// Unsubscribe from an event type
    pub async fn unsubscribe(&self, listener_id: Uuid) -> Result<()> {
        let mut listeners = self.listeners.write().await;
        
        for (_, listener_list) in listeners.iter_mut() {
            listener_list.retain(|listener| listener.id != listener_id);
        }
        
        Ok(())
    }

    /// Publish an event to all interested listeners
    pub async fn publish(&self, event: Event) -> Result<()> {
        let listeners = self.listeners.read().await;
        
        if let Some(event_listeners) = listeners.get(&event.event_type) {
            for listener in event_listeners {
                if listener.is_interested_in(&event) {
                    // Clone the event and listener for the async task
                    let event_clone = event.clone();
                    let listener_clone = listener.clone();
                    
                    // Spawn a task to handle the event
                    tokio::spawn(async move {
                        if let Err(e) = listener_clone.handle_event(event_clone).await {
                            log::error!("Error handling event: {:?}", e);
                        }
                    });
                }
            }
        }
        
        Ok(())
    }

    /// Get the number of listeners for a specific event type
    pub async fn listener_count(&self, event_type: EventType) -> usize {
        let listeners = self.listeners.read().await;
        listeners.get(&event_type).map_or(0, |v| v.len())
    }

    /// Get the total number of listeners across all event types
    pub async fn total_listener_count(&self) -> usize {
        let listeners = self.listeners.read().await;
        listeners.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::event::Event;
    use std::future::Future;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::pin::Pin;

    
    #[tokio::test]
    async fn test_event_subscription() {
        let event_system = EventSystem::new();
        
        let callback = Arc::new(|_event: Event| {
            Box::pin(async { Ok(()) }) as Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>
        });
        
        let listener = EventListener::new(EventType::TaskCompleted, None, callback);
        
        let listener_id = event_system.subscribe(listener).await.unwrap();
        assert_eq!(event_system.listener_count(EventType::TaskCompleted).await, 1);
        
        event_system.unsubscribe(listener_id).await.unwrap();
        assert_eq!(event_system.listener_count(EventType::TaskCompleted).await, 0);
    }
    
    #[tokio::test]
    async fn test_event_publishing() {
        let event_system = EventSystem::new();
        let event_received = Arc::new(AtomicBool::new(false));
        
        let event_received_clone = event_received.clone();
        let callback = Arc::new(move |_event: Event| {
            let flag = event_received_clone.clone();
            Box::pin(async move {
                flag.store(true, Ordering::SeqCst);
                Ok(())
            }) as Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>
        });
        
        let listener = EventListener::new(EventType::TaskCompleted, None, callback);
        event_system.subscribe(listener).await.unwrap();
        
        let task_id = Uuid::new_v4();
        let event = Event::task_completed(task_id);
        event_system.publish(event).await.unwrap();
        
        // Give some time for the async handler to execute
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        assert!(event_received.load(Ordering::SeqCst));
    }
}