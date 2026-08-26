use std::time::Duration;

use atb_cli_utils::AtbCli;
use atb_tokio_ext::shutdown_signal;
use {{ project-name | snake_case }}_core::temporal::{self, HealthCheckActivities, HealthCheckWorkflow};
use temporalio_client::Client;
use temporalio_sdk::{Runtime, Worker, WorkerOptions};

use crate::opts::WorkerOpts;

pub async fn run(opts: WorkerOpts) -> anyhow::Result<()> {
    let client = temporal::try_connect_temporal(
        &opts.temporal.temporal,
        &opts.temporal.namespace,
        Duration::from_secs(30),
    )
    .await?;

    // A worker owns its current-thread runtime; scale with more processes or worker tuning.
    let worker_options = worker_options(&opts)?;
    let handle = std::thread::spawn(move || start_worker(client, worker_options));

    handle
        .join()
        .map_err(|error| anyhow::anyhow!("worker thread panicked: {error:?}"))??;

    Ok(())
}

pub fn worker_options(opts: &WorkerOpts) -> anyhow::Result<WorkerOptions> {
    let client_id = crate::Cli::client_id();
    WorkerOptions::new(opts.temporal.task_queue.clone())
        .client_identity_override(client_id)
        .max_cached_workflows(opts.max_cached_workflows)
        .register_workflow::<HealthCheckWorkflow>()
        .map(|options| options.register_activities(HealthCheckActivities).build())
        .map_err(Into::into)
}

pub fn start_worker(client: Client, worker_options: WorkerOptions) -> anyhow::Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            let runtime = Runtime::new_assume_tokio(Default::default())?;
            let mut worker = Worker::new(&runtime, client, worker_options)?;
            let shutdown_worker = worker.shutdown_handle();
            let shutdown_task = tokio::spawn(async move {
                shutdown_signal().await;
                tracing::info!("Temporal worker shutting down from signal");
                shutdown_worker();
            });

            tracing::info!("Temporal worker starting");
            let result = worker.run().await;
            shutdown_task.abort();
            result?;
            tracing::info!("Temporal worker stopped");
            Ok(())
        })
}
