use std::{str::FromStr, time::Duration};

use atb_types::Uuid;
use temporalio_client::{Client, ClientOptions, ConnectionOptions, Url, WorkflowStartOptions};
use temporalio_common::protos::temporal::api::enums::v1::WorkflowIdReusePolicy;
use temporalio_macros::{activities, workflow, workflow_methods};
use temporalio_sdk::{
    ActivityOptions, WorkflowContext, WorkflowResult,
    activities::{ActivityContext, ActivityError},
};
use tokio::time;

pub const WF_HEALTH_CHECK: &str = "health_check";

#[derive(Clone)]
pub struct WorkflowEngine {
    pub client: Client,
    pub task_queue: String,
}

impl WorkflowEngine {
    pub fn new(client: Client, task_queue: impl Into<String>) -> Self {
        Self {
            client,
            task_queue: task_queue.into(),
        }
    }

    pub async fn start_health_check(&self) -> anyhow::Result<WorkflowExecution> {
        let workflow_id = format!("{WF_HEALTH_CHECK}_{}", Uuid::now_v7());
        let options = WorkflowStartOptions::new(self.task_queue.clone(), workflow_id.clone())
            .id_reuse_policy(WorkflowIdReusePolicy::RejectDuplicate)
            .build();
        let handle = self
            .client
            .start_workflow(HealthCheckWorkflow::run, (), options)
            .await?;

        Ok(WorkflowExecution {
            workflow_id,
            run_id: handle.run_id().map(ToOwned::to_owned),
        })
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowExecution {
    pub workflow_id: String,
    pub run_id: Option<String>,
}

/// Create a native Temporal client, retrying until the timeout elapses.
pub async fn try_connect_temporal(
    temporal_url: &str,
    namespace: &str,
    timeout: Duration,
) -> anyhow::Result<Client> {
    let connection_options = ConnectionOptions::new(Url::from_str(temporal_url)?)
        .connect_timeout(timeout)
        .build();
    let client_options = ClientOptions::new(namespace).build();
    let timeout_fut = time::sleep(timeout);
    tokio::pin!(timeout_fut);

    loop {
        tokio::select! {
            result = Client::connect(connection_options.clone(), client_options.clone()) => {
                match result {
                    Ok(client) => return Ok(client),
                    Err(error) => tracing::error!(%error, "Temporal connection failed"),
                }
            }
            _ = &mut timeout_fut => {
                return Err(anyhow::anyhow!("Temporal client connection attempts timed out"));
            }
        }

        tracing::info!("waiting for Temporal");
        time::sleep(Duration::from_secs(1)).await;
    }
}

// ----- Example workflow and activity ------------------------------------

pub struct HealthCheckActivities;

#[activities]
impl HealthCheckActivities {
    #[activity(name = "health_check")]
    pub async fn health_check(_ctx: ActivityContext, payload: u32) -> Result<u32, ActivityError> {
        tracing::info!(payload, "health check activity");
        Ok(payload)
    }
}

#[workflow]
#[derive(Default)]
pub struct HealthCheckWorkflow;

#[workflow_methods]
impl HealthCheckWorkflow {
    #[run(name = "health_check")]
    pub async fn run(ctx: &mut WorkflowContext<Self>) -> WorkflowResult<()> {
        let pong = ctx
            .execute_activity(
                HealthCheckActivities::health_check,
                42_u32,
                ActivityOptions::start_to_close_timeout(Duration::from_secs(10)),
            )
            .await?;
        tracing::info!(pong, "health check workflow completed");
        Ok(())
    }
}

#[cfg(all(test, feature = "temporal-tests"))]
mod tests {
    use super::{HealthCheckActivities, HealthCheckWorkflow};
    use atb_types::Uuid;
    use temporalio_client::{WorkflowGetResultOptions, WorkflowStartOptions};
    use temporalio_sdk::{
        Runtime, Worker, WorkerOptions,
        testing::{ActivityEnvironment, LocalWorkflowEnvironmentOptions, WorkflowEnvironment},
    };

    const TASK_QUEUE: &str = "{{ project-name | kebab_case }}-template-tests";

    #[tokio::test]
    async fn activity_completes() {
        let environment = ActivityEnvironment::builder().build();
        let result = environment
            .run(HealthCheckActivities::health_check, 42_u32)
            .await
            .expect("activity result");

        assert_eq!(result, 42);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "downloads and starts the Temporal CLI dev server"]
    async fn workflow_completes() {
        let environment =
            WorkflowEnvironment::start_local(LocalWorkflowEnvironmentOptions::default())
                .await
                .expect("start Temporal test environment");
        let runtime = Runtime::new_assume_tokio(Default::default()).expect("Temporal runtime");
        let worker_options = WorkerOptions::new(TASK_QUEUE)
            .register_workflow::<HealthCheckWorkflow>()
            .expect("register workflow")
            .register_activities(HealthCheckActivities)
            .build();
        let mut worker = Worker::new(&runtime, environment.client().clone(), worker_options)
            .expect("create worker");
        let shutdown = worker.shutdown_handle();
        let workflow = async {
            let workflow_id = format!("health-check-{}", Uuid::new_v4());
            let handle = environment
                .client()
                .start_workflow(
                    HealthCheckWorkflow::run,
                    (),
                    WorkflowStartOptions::new(TASK_QUEUE, workflow_id).build(),
                )
                .await
                .expect("start workflow");
            let result: () = handle
                .get_result(WorkflowGetResultOptions::default())
                .await
                .expect("workflow result");
            shutdown();
            result
        };

        let (worker_result, ()) = tokio::join!(worker.run(), workflow);
        worker_result.expect("worker run");
        environment.shutdown().await.expect("stop test environment");
    }
}
