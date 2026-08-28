//! A trivial in-process `Bus` implementation.
//!
//! This is the single-node Phase-A realization; the same `Bus` trait is
//! reused by the multinode transport (Phase C) — only the transport layer
//! changes, not the command/query vocabulary.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::{Bus, BusError, Command, Query, QueryValue};

/// Minimal in-process state for testing / single-node operation.
#[derive(Default)]
struct State {
    task_counter: AtomicU64,
    // placeholder results
    results: Vec<String>,
}

/// In-process bus — not for production multinode use.
pub struct InprocBus {
    state: Arc<Mutex<State>>,
}

impl Default for InprocBus {
    fn default() -> Self {
        Self::new()
    }
}

impl InprocBus {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
        }
    }

    pub async fn next_task_id(&self) -> arachne_domain::TaskId {
        let id = self
            .state
            .lock()
            .await
            .task_counter
            .fetch_add(1, Ordering::SeqCst);
        arachne_domain::TaskId::new(id)
    }
}

#[async_trait]
impl Bus for InprocBus {
    async fn dispatch(&self, _cmd: Command) -> Result<(), BusError> {
        // Phase A: accept all commands, mutate placeholder state.
        match _cmd {
            Command::RunTask { .. } => Ok(()),
            Command::Escalate { .. } => Err(BusError::NoCommandHandler),
            Command::RotateProxy { .. } => Ok(()),
            Command::CancelTask { .. } => Ok(()),
            Command::Checkpoint {} => Ok(()),
        }
    }

    async fn query(&self, q: Query) -> Result<QueryValue, BusError> {
        match q {
            Query::GetTaskStatus { .. } => {
                Ok(QueryValue::Json(serde_json::json!({ "status": "ok" })))
            }
            Query::ListWorkers {} => Ok(QueryValue::Json(serde_json::json!([{"id": "worker-0"}]))),
            Query::GetResults { offset, limit, .. } => {
                let s = self.state.lock().await;
                let slice: Vec<&String> = s
                    .results
                    .iter()
                    .skip(offset as usize)
                    .take(limit as usize)
                    .collect();
                Ok(QueryValue::Json(
                    serde_json::to_value(slice).map_err(|e| BusError::Internal(e.to_string()))?,
                ))
            }
        }
    }
}
