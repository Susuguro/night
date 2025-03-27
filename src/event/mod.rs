//! Event system module for Night.
//!
//! This module provides a simple event system to replace the complex TCP-based
//! communication mechanism. It implements an in-memory publish-subscribe pattern
//! for task status notifications.

pub mod event;
pub mod listener;
pub mod system;

pub use event::Event;
pub use event::EventType;
pub use listener::EventListener;
pub use listener::ListenerCallback;
pub use system::EventSystem;