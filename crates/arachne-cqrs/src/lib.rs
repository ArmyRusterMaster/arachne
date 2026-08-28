//! CQRS bus for Arachne.
//!
//! Commands change state; queries read data — both go through a single
//! `Bus` trait so the same abstractions work in single-node and later
//! distributed (multinode) modes.  See `docs/05-rust-patterns.md` §5.8.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use arachne_domain::{SessionId, TaskId};

/// A state-changing command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    /// Enqueue a new crawl task from a job descriptor.
    RunTask { task: TaskId },
    /// Escalate a fast HTTP session to full engine mode (Phase B — placeholder).
    Escalate { session: SessionId },
    /// Rotate the proxy for a given session.
    RotateProxy { session: SessionId },
    /// Cancel a running task.
    CancelTask { task: TaskId },
    /// Checkpoint the URL queue to disk (for kill -9 resume).
    Checkpoint {},
}

/// A read-only query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Query {
    /// Get the current status of a task.
    GetTaskStatus { task: TaskId },
    /// List active workers / sessions.
    ListWorkers {},
    /// Get the result rows for a task (paginated).
    GetResults {
        task: TaskId,
        offset: u64,
        limit: u32,
    },
}

/// Value returned from a query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryValue {
    Json(serde_json::Value),
    Empty,
}

/// Errors raised by the bus.
#[derive(Debug, Error)]
pub enum BusError {
    #[error("handler not registered for command")]
    NoCommandHandler,
    #[error("handler not registered for query")]
    NoQueryHandler,
    #[error("internal: {0}")]
    Internal(String),
}

/// Unified entry point — the bus dispatches commands (state changes)
/// and answers queries (reads).  A trivial in-process implementation
/// is provided in `inproc`; multinode uses the same trait over a queue.
#[async_trait]
pub trait Bus: Send + Sync {
    async fn dispatch(&self, cmd: Command) -> Result<(), BusError>;
    async fn query(&self, q: Query) -> Result<QueryValue, BusError>;
}

pub mod inproc;

#[cfg(test)]
mod tests;
