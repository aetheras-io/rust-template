use std::{str::FromStr, time::Duration};

use atb_temporal_ext::activity;
use atb_types::Uuid;
use temporalio_client::WorkflowOptions;
pub use temporalio_common::{
    protos::temporal::api::enums::v1::WorkflowIdReusePolicy,
    telemetry::TelemetryOptions,
    worker::{WorkerConfig, WorkerConfigBuilder, WorkerTaskTypes, WorkerVersioningStrategy},
};
pub use temporalio_sdk::{
    ActContext, ActExitValue, ActivityError, WfContext, Worker, WorkflowResult, sdk_client_options,
};
pub use temporalio_sdk_core::{
    Client, CoreRuntime, RetryClient, RuntimeOptions, Url, WorkflowClientTrait, init_worker,
};
use tokio::time;

pub type TemporalClient = RetryClient<Client>;
pub type ActivityResult<T, E = ActivityError> = std::result::Result<T, E>;

pub const WF_HEALTH_CHECK: &str = "health_check";

#[derive(Clone)]
pub struct WorkflowEngine {
    pub client: TemporalClient,
    pub task_queue: String,
}

impl WorkflowEngine {
    pub fn new(client: TemporalClient, task_queue: impl Into<String>) -> Self {
        Self {
            client,
            task_queue: task_queue.into(),
        }
    }

    pub async fn start_health_check(&self) -> anyhow::Result<WorkflowExecution> {
        let workflow_id = format!("{WF_HEALTH_CHECK}_{}", Uuid::now_v7());
        let response = self
            .client
            .start_workflow(
                vec![],
                self.task_queue.clone(),
                workflow_id.clone(),
                WF_HEALTH_CHECK.to_string(),
                None,
                WorkflowOptions {
                    id_reuse_policy: WorkflowIdReusePolicy::RejectDuplicate,
                    ..Default::default()
                },
            )
            .await?;

        Ok(WorkflowExecution {
            workflow_id,
            run_id: Some(response.run_id),
        })
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowExecution {
    pub workflow_id: String,
    pub run_id: Option<String>,
}

/// Create a Temporal client, retrying until the timeout elapses.
pub async fn try_connect_temporal(
    temporal_url: &str,
    namespace: &str,
    timeout: Duration,
) -> anyhow::Result<TemporalClient> {
    let server_options = sdk_client_options(Url::from_str(temporal_url)?).build();
    let timeout_fut = time::sleep(timeout);
    tokio::pin!(timeout_fut);
    loop {
        tokio::select! {
            res = server_options.connect(namespace, None) => {
                match res {
                    Ok(c) => return Ok(c),
                    Err(e) => tracing::error!("temporal connection failed: {}", e),
                }
            }
            _ = &mut timeout_fut => {
                return Err(anyhow::anyhow!("Temporal Client connection attempts timed out"))
            }
        }

        tracing::info!("waiting for temporal...");
        time::sleep(Duration::from_secs(1)).await;
    }
}

pub struct TemporalWorker {
    inner: Worker,
}

impl TemporalWorker {
    pub fn new(mut worker: Worker) -> anyhow::Result<Self> {
        worker.register_wf(WF_HEALTH_CHECK, health_check_workflow);
        HealthCheckActivity::bind(&mut worker);

        Ok(Self { inner: worker })
    }

    pub fn inner_mut(&mut self) -> &mut Worker {
        &mut self.inner
    }

    pub async fn run(
        self,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> anyhow::Result<()> {
        tracing::info!("TemporalWorker starting");
        let mut worker = self.inner;
        let worker_shutdown_handle = worker.shutdown_handle();
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                res = worker.run() => {
                    tracing::info!("TemporalWorker stopped: {:?}", res);
                    match res {
                        Ok(_) => continue,
                        Err(e) => {
                            tracing::warn!("TemporalWorker Failed: {:?}", e);
                            return Err(e);
                        }
                    }
                },

                _ = &mut shutdown => {
                    tracing::warn!("TemporalWorker shutting down from signal");
                    worker_shutdown_handle();
                    break;
                }
            }
        }
        tracing::info!("TemporalWorker stopped");
        Ok(())
    }
}

// ----- Example workflow & activity ---------------------------------------

pub async fn health_check_workflow(ctx: WfContext) -> WorkflowResult<()> {
    let ping = 43;
    tracing::info!("health_check_workflow ping {ping}");
    let pong = health_check(&ctx, &42)?.run().await?;
    tracing::info!("health_check_workflow pong {pong}");
    let pong = health_check(&ctx, &42)?.run().await?;
    tracing::info!("health_check_workflow pong2 {pong}");
    Ok(().into())
}

#[activity]
pub async fn health_check(_ctx: ActContext, payload: u32) -> ActivityResult<ActExitValue<u32>> {
    tracing::info!("health_check_activity ping {payload}");
    Ok(payload.into())
}

// ----- Test helpers -------------------------------------------------------
// Uses the Temporal dev server from docker-compose (localhost:7233 by default).
// Marked ignored so it only runs when explicitly requested:
//   cargo test -p {{ project-name | snake_case }}-core temporal::tests::workflow_activity_override_example -- --ignored
#[cfg(all(test, feature = "temporal-tests"))]
mod tests {
    #![allow(unused)]

    use super::*;
    use anyhow::anyhow;
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Arc, Mutex, OnceLock, Weak};
    use std::thread;
    use temporalio_client::{ClientOptions, WfClientExt, WorkflowExecutionResult, WorkflowOptions};
    use temporalio_sdk::Worker;
    use temporalio_sdk_core::ephemeral_server::{TestServerConfig, default_cached_download};
    use tokio::sync::oneshot;

    const TASK_QUEUE: &str = "{{ project-name | kebab_case }}-template-tests";
    const HEALTH_CHECK_ACTIVITY: &str = "health_check";

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ActivityOutcome {
        Succeed,
        FailRetryable(String),
        FailNonRetryable(String),
    }

    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    struct ScenarioKey {
        workflow_id: String,
        activity_type: String,
    }

    fn scenarios() -> &'static Mutex<HashMap<ScenarioKey, VecDeque<ActivityOutcome>>> {
        static SCENARIOS: OnceLock<Mutex<HashMap<ScenarioKey, VecDeque<ActivityOutcome>>>> =
            OnceLock::new();
        SCENARIOS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn set_activity_scenario(
        workflow_id: impl Into<String>,
        activity_type: impl Into<String>,
        outcomes: impl IntoIterator<Item = ActivityOutcome>,
    ) {
        let key = ScenarioKey {
            workflow_id: workflow_id.into(),
            activity_type: activity_type.into(),
        };

        scenarios()
            .lock()
            .expect("scenario lock")
            .insert(key, outcomes.into_iter().collect());
    }

    fn next_outcome(workflow_id: &str, activity_type: &str) -> Option<ActivityOutcome> {
        let mut guard = scenarios().lock().expect("scenario lock");
        let key = ScenarioKey {
            workflow_id: workflow_id.to_owned(),
            activity_type: activity_type.to_owned(),
        };
        let Some(queue) = guard.get_mut(&key) else {
            return None;
        };
        let outcome = queue.pop_front();
        if queue.is_empty() {
            guard.remove(&key);
        }
        outcome
    }

    async fn interceptable_health_check(
        ctx: ActContext,
        payload: u32,
    ) -> ActivityResult<ActExitValue<u32>> {
        let info = ctx.get_info();
        if let Some(wf) = &info.workflow_execution {
            if let Some(outcome) = next_outcome(&wf.workflow_id, &info.activity_type) {
                return match outcome {
                    ActivityOutcome::Succeed => HealthCheckActivity::handler(ctx, payload).await,
                    ActivityOutcome::FailRetryable(msg) => Err(ActivityError::Retryable {
                        source: anyhow!(msg),
                        explicit_delay: None,
                    }),
                    ActivityOutcome::FailNonRetryable(msg) => {
                        Err(ActivityError::NonRetryable(anyhow!(msg)))
                    }
                };
            }
        }

        HealthCheckActivity::handler(ctx, payload).await
    }

    struct Harness {
        client: TemporalClient,
        task_queue: String,
        worker: Option<thread::JoinHandle<()>>,
        shutdown: Option<Arc<dyn Fn() + Send + Sync + 'static>>,
    }

    impl Harness {
        /// Start or reuse the shared ephemeral Temporal server for all tests.
        ///
        /// We keep only a weak pointer in the global slot so the harness is
        /// dropped (and the server shut down) when the last test finishes.
        async fn init() -> Arc<Self> {
            static SLOT: OnceLock<Mutex<Weak<Harness>>> = OnceLock::new();
            let slot = SLOT.get_or_init(|| Mutex::new(Weak::new()));

            // Fast path: upgrade existing harness if still alive.
            if let Some(existing) = slot.lock().unwrap().upgrade() {
                return existing;
            }

            // Slow path: start a new ephemeral server and worker.
            let config = TestServerConfig::builder()
                .exe(default_cached_download())
                .build();
            let server = config.start_server().await.expect("start test server");
            let target = format!("http://{}", server.target);
            let client: TemporalClient = ClientOptions::builder()
                .identity("{{ project-name | kebab_case }}-core-tests".to_string())
                .target_url(Url::parse(&target).expect("test server url"))
                .client_name("{{ project-name | kebab_case }}-core".to_string())
                .client_version(env!("CARGO_PKG_VERSION").to_string())
                .build()
                .connect("default", None)
                .await
                .expect("connect client");
            let (shutdown_tx, shutdown_rx) = oneshot::channel();

            let worker_client = client.clone();
            let handle = thread::spawn(move || {
                let client = worker_client.clone();
                // Move the ephemeral server into the worker thread so shutdown happens
                // on the same runtime that started it.
                let mut server = Some(server);

                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("worker runtime")
                    .block_on(async move {
                        let telemetry = TelemetryOptions::builder().build();
                        let runtime = CoreRuntime::new_assume_tokio(
                            RuntimeOptions::builder()
                                .telemetry_options(telemetry)
                                .build()
                                .expect("runtime opts"),
                        )
                        .expect("runtime");

                        let worker_cfg = WorkerConfig::builder()
                            .namespace("default")
                            .task_queue(TASK_QUEUE)
                            .task_types(WorkerTaskTypes::all())
                            .versioning_strategy(WorkerVersioningStrategy::None {
                                build_id: "unit-test-worker".to_owned(),
                            })
                            .skip_client_worker_set_check(true)
                            .build()
                            .expect("worker cfg");

                        let core_worker = Arc::new(
                            init_worker(&runtime, worker_cfg, client).expect("init worker"),
                        );
                        let mut worker = Worker::new_from_core(core_worker, TASK_QUEUE.to_string());
                        worker.register_wf(WF_HEALTH_CHECK, health_check_workflow);
                        worker.register_activity(HEALTH_CHECK_ACTIVITY, interceptable_health_check);
                        let shutdown = worker.shutdown_handle();
                        let _ = shutdown_tx.send(shutdown);
                        worker.run().await.expect("worker run");

                        // After the worker stops, shut down the ephemeral server from this runtime.
                        if let Some(mut srv) = server.take() {
                            let _ = srv.shutdown().await;
                        }
                    });
            });

            let shutdown = shutdown_rx.await.expect("recv shutdown");

            let harness = Arc::new(Harness {
                client,
                task_queue: TASK_QUEUE.to_string(),
                worker: Some(handle),
                shutdown: Some(Arc::new(shutdown)),
            });

            *slot.lock().unwrap() = Arc::downgrade(&harness);
            harness
        }
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            if let Some(shutdown) = self.shutdown.take() {
                shutdown();
            }
            if let Some(handle) = self.worker.take() {
                let _ = handle.join();
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn workflow_completes() {
        let h = Harness::init().await;
        let wf_id = format!("wf-{}", Uuid::new_v4());

        let run = h
            .client
            .start_workflow(
                vec![],
                h.task_queue.clone(),
                wf_id.clone(),
                WF_HEALTH_CHECK.to_string(),
                None,
                WorkflowOptions::default(),
            )
            .await
            .expect("start workflow");

        let handle = h
            .client
            .get_untyped_workflow_handle(wf_id, run.run_id.clone());
        let result = handle
            .get_workflow_result(Default::default())
            .await
            .expect("workflow result");

        assert!(matches!(result, WorkflowExecutionResult::Succeeded(_)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn workflow_fails_when_first_health_check_fails() {
        let h = Harness::init().await;
        let wf_id = format!("wf-first-fail-{}", Uuid::new_v4());

        set_activity_scenario(
            &wf_id,
            HEALTH_CHECK_ACTIVITY,
            [
                ActivityOutcome::FailNonRetryable("first call fails".into()),
                ActivityOutcome::Succeed,
            ],
        );

        let run = h
            .client
            .start_workflow(
                vec![],
                h.task_queue.clone(),
                wf_id.clone(),
                WF_HEALTH_CHECK.to_string(),
                None,
                WorkflowOptions::default(),
            )
            .await
            .expect("start workflow");

        let handle = h
            .client
            .get_untyped_workflow_handle(wf_id, run.run_id.clone());
        let result = handle
            .get_workflow_result(Default::default())
            .await
            .expect("workflow result");

        assert!(matches!(result, WorkflowExecutionResult::Failed(_)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn workflow_fails_when_second_health_check_fails() {
        let h = Harness::init().await;
        let wf_id = format!("wf-second-fail-{}", Uuid::new_v4());

        set_activity_scenario(
            &wf_id,
            HEALTH_CHECK_ACTIVITY,
            [
                ActivityOutcome::Succeed,
                ActivityOutcome::FailNonRetryable("second call fails".into()),
            ],
        );

        let run = h
            .client
            .start_workflow(
                vec![],
                h.task_queue.clone(),
                wf_id.clone(),
                WF_HEALTH_CHECK.to_string(),
                None,
                WorkflowOptions::default(),
            )
            .await
            .expect("start workflow");

        let handle = h
            .client
            .get_untyped_workflow_handle(wf_id, run.run_id.clone());
        let result = handle
            .get_workflow_result(Default::default())
            .await
            .expect("workflow result");

        assert!(matches!(result, WorkflowExecutionResult::Failed(_)));
    }
}
