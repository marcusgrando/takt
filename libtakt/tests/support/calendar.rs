use super::*;

// ── MockBridge ────────────────────────────────────────────────────────

pub(super) struct MockBridge {
    pub(super) access_status: Mutex<CalendarAccessStatus>,
    pub(super) calendars: Mutex<Vec<CalendarInfo>>,
    pub(super) events_in_window: Mutex<Vec<CalendarEvent>>,
    pub(super) event_instance: Mutex<Option<CalendarEvent>>,
    pub(super) fetched_instance: tokio::sync::Notify,
}

impl MockBridge {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            access_status: Mutex::new(CalendarAccessStatus::Authorized),
            calendars: Mutex::new(vec![]),
            events_in_window: Mutex::new(vec![]),
            event_instance: Mutex::new(None),
            fetched_instance: tokio::sync::Notify::new(),
        })
    }

    pub(super) fn with_event_instance(self: Arc<Self>, event: CalendarEvent) -> Arc<Self> {
        *self.event_instance.lock().unwrap() = Some(event);
        self
    }
}

impl PlatformBridge for MockBridge {
    fn send_notification(&self, _: String, _: String, _: bool) {}
    fn run_on_main_sync(&self, _: u64) {}
    fn get_user_activity_snapshot(&self) -> UserActivitySnapshot {
        UserActivitySnapshot {
            session_active: true,
            eligibility_generation: 0,
            input_event_count: 0,
            eligibility_input_event_count: 0,
            last_input_at_unix_millis: None,
        }
    }
    fn get_calendar_access_status(
        &self,
    ) -> Result<CalendarAccessStatus, libtakt::error::TaktError> {
        Ok(self.access_status.lock().unwrap().clone())
    }
    fn request_calendar_access(&self) -> Result<CalendarAccessStatus, libtakt::error::TaktError> {
        Ok(CalendarAccessStatus::Authorized)
    }
    fn list_calendars(&self) -> Result<Vec<CalendarInfo>, libtakt::error::TaktError> {
        Ok(self.calendars.lock().unwrap().clone())
    }
    fn fetch_events_in_window(
        &self,
        _: String,
        _: u32,
        _: u32,
    ) -> Result<Vec<CalendarEvent>, libtakt::error::TaktError> {
        Ok(self.events_in_window.lock().unwrap().clone())
    }
    fn fetch_event_instance(
        &self,
        _: String,
        _: String,
        _: String,
    ) -> Result<Option<CalendarEvent>, libtakt::error::TaktError> {
        self.fetched_instance.notify_one();
        Ok(self.event_instance.lock().unwrap().clone())
    }
}

// ── SpyExecutor ───────────────────────────────────────────────────────

#[derive(Default, Clone)]
pub(super) struct SpyExecutor {
    pub(super) calls: Arc<Mutex<Vec<(Action, Option<CalendarEvent>)>>>,
}

#[async_trait::async_trait]
impl ActionExecutor for SpyExecutor {
    async fn execute(
        &self,
        action: &Action,
        event: Option<&CalendarEvent>,
    ) -> Result<ExecutionResult, ExecutorError> {
        self.calls
            .lock()
            .unwrap()
            .push((action.clone(), event.cloned()));
        Ok(ExecutionResult {
            stdout: None,
            stderr: None,
        })
    }
}

// ── Fixtures ──────────────────────────────────────────────────────────

pub(super) async fn make_store(bridge: Arc<dyn PlatformBridge>) -> Arc<TaskStore> {
    make_store_with_pool(bridge).await.0
}

pub(super) async fn make_store_with_pool(
    bridge: Arc<dyn PlatformBridge>,
) -> (Arc<TaskStore>, sqlx::SqlitePool) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .expect("pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrate");
    (Arc::new(TaskStore::new(pool.clone(), bridge)), pool)
}

pub(super) fn sample_event(
    id: &str,
    calendar_id: &str,
    start_iso: &str,
    title: &str,
) -> CalendarEvent {
    CalendarEvent {
        id: id.into(),
        title: title.into(),
        start: start_iso.into(),
        end: start_iso.into(),
        notes: None,
        location: None,
        url: None,
        conference_url: Some("https://meet.test/abc".into()),
        calendar_id: calendar_id.into(),
    }
}

/// Create a Calendar-scheduled task in the store. Returns the auto-generated task ID.
pub(super) async fn create_calendar_task(
    store: &Arc<TaskStore>,
    calendar_id: &str,
    run_if_missed: bool,
) -> String {
    let task = store
        .create_task(
            "test-task".into(),
            None,
            run_if_missed,
            false,
            Schedule::Calendar {
                calendar_id: calendar_id.into(),
                title_contains: None,
                minutes_before: 0,
            },
            Action::Settings {
                pane_url: "x".into(),
            },
        )
        .await
        .expect("create_task");
    task.id
}
