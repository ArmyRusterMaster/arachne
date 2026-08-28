//! Tests for arachne-cqrs InprocBus.

use super::*;
use crate::inproc::InprocBus;

#[tokio::test]
async fn run_task_command_accepted() {
    let bus = InprocBus::new();
    let t = bus.next_task_id().await;
    let cmd = Command::RunTask { task: t };
    assert!(bus.dispatch(cmd).await.is_ok());
}

#[tokio::test]
async fn escalate_returns_no_handler() {
    // Phase A does not implement escalation (Phase B feature).
    let bus = InprocBus::new();
    let sid = arachne_domain::SessionId::new();
    let cmd = Command::Escalate { session: sid };
    assert!(matches!(
        bus.dispatch(cmd).await,
        Err(BusError::NoCommandHandler)
    ));
}

#[tokio::test]
async fn list_workers_query_returns_json_array() {
    let bus = InprocBus::new();
    let q = Query::ListWorkers {};
    let val = bus.query(q).await.unwrap();
    match val {
        QueryValue::Json(v) => {
            assert!(v.is_array());
            assert!(!v.as_array().unwrap().is_empty());
        }
        _ => panic!("expected Json"),
    }
}

#[tokio::test]
async fn get_task_status_returns_ok() {
    let bus = InprocBus::new();
    let q = Query::GetTaskStatus {
        task: arachne_domain::TaskId::new(0),
    };
    let val = bus.query(q).await.unwrap();
    match val {
        QueryValue::Json(v) => assert_eq!(v["status"], "ok"),
        _ => panic!("expected Json"),
    }
}

#[tokio::test]
async fn get_results_pagination() {
    let bus = InprocBus::new();
    let q = Query::GetResults {
        task: arachne_domain::TaskId::new(0),
        offset: 0,
        limit: 10,
    };
    let val = bus.query(q).await.unwrap();
    match val {
        QueryValue::Json(v) => assert!(v.is_array()),
        _ => panic!("expected Json"),
    }
}
