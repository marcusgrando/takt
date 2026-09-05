use crate as libtakt;
use crate::executor::{ActionExecutor, ExecutionResult, ExecutorError};
use crate::models::*;
use crate::platform::PlatformBridge;
use crate::store::TaskStore;
use std::sync::{Arc, Mutex};

#[path = "../tests/support/calendar.rs"]
mod support;
use support::*;

#[tokio::test]
async fn startup_recovery_and_poller_execute_occurrence_once() {
    let start = chrono::Utc::now().to_rfc3339();
    let event = sample_event("e", "cal", &start, "Meeting");
    let bridge = MockBridge::new().with_event_instance(event.clone());
    *bridge.events_in_window.lock().unwrap() = vec![event];
    let store = make_store(bridge.clone()).await;
    let task = create_calendar_task(&store, "cal", true).await;
    store
        .insert_calendar_dispatch(&task, "e", &start, "scheduled", &start)
        .await
        .unwrap();
    let executor = Arc::new(SpyExecutor::default());
    let scheduler = crate::scheduler::AppScheduler::new(store.clone(), executor.clone(), bridge)
        .await
        .unwrap();
    scheduler.load_all_tasks().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if !store.list_logs(Some(&task), 10).await.unwrap().is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    tokio::time::resume();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(executor.calls.lock().unwrap().len(), 1);
    assert_eq!(store.list_logs(Some(&task), 10).await.unwrap().len(), 1);
    assert!(store.list_pending_dispatches().await.unwrap().is_empty());
}
