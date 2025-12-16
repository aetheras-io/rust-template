use crate::opts::WorkerOpts;
use std::{sync::Arc, time::Duration};

use {{ project-name | snake_case }}_core::temporal::{
    self, CoreRuntime, RuntimeOptions, TelemetryOptions, TemporalClient, TemporalWorker, Worker,
    WorkerConfig, WorkerTaskTypes, WorkerVersioningStrategy, init_worker,
};

use atb_cli_utils::AtbCli;
use atb_tokio_ext::shutdown_signal;

pub async fn run(opts: WorkerOpts) -> anyhow::Result<()> {
    let client = temporal::try_connect_temporal(
        &opts.temporal.temporal,
        &opts.temporal.namespace,
        Duration::from_secs(30),
    )
    .await?;

    // Single worker entity per process; scale via pollers/outstanding task limits.
    let worker_config = worker_config(&opts)?;
    let handle = std::thread::spawn(move || start_worker(client, worker_config));

    handle
        .join()
        .map_err(|e| anyhow::anyhow!("worker thread panicked: {:?}", e))??;

    Ok(())
}

pub fn worker_config(opts: &WorkerOpts) -> anyhow::Result<WorkerConfig> {
    let client_id = crate::Cli::client_id();
    WorkerConfig::builder()
        .namespace(opts.temporal.namespace.clone())
        .task_queue(opts.temporal.task_queue.clone())
        .task_types(WorkerTaskTypes::all())
        .client_identity_override(client_id.clone())
        .versioning_strategy(WorkerVersioningStrategy::None {
            build_id: client_id,
        })
        .max_cached_workflows(opts.max_cached_workflows)
        .build()
        .map_err(|s| anyhow::anyhow!("{s}"))
}

pub fn start_worker(client: TemporalClient, worker_config: WorkerConfig) -> anyhow::Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            let worker = new_worker(client, worker_config)?;
            worker.run(shutdown_signal()).await?;
            Ok(())
        })
}

pub fn new_worker(
    client: TemporalClient,
    worker_config: WorkerConfig,
) -> anyhow::Result<TemporalWorker> {
    let telemetry_options = TelemetryOptions::builder().build();
    let runtime_options = RuntimeOptions::builder()
        .telemetry_options(telemetry_options)
        .build()
        .map_err(|s| anyhow::anyhow!("{s}"))?;

    let runtime = CoreRuntime::new_assume_tokio(runtime_options)?;
    let task_queue = worker_config.task_queue.clone();
    let core_worker = Arc::new(init_worker(&runtime, worker_config, client)?);
    let worker = Worker::new_from_core(core_worker, task_queue);

    TemporalWorker::new(worker)
}
