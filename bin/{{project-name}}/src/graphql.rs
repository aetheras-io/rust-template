use async_graphql::{Context, EmptySubscription, Object, Result, Schema, SchemaBuilder};

use {{ project-name | snake_case }}_core::temporal::WorkflowEngine;
use sqlx::PgPool;

pub type AppSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

pub fn schema() -> SchemaBuilder<QueryRoot, MutationRoot, EmptySubscription> {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
}

#[derive(Default)]
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn version(&self, ctx: &Context<'_>) -> Result<String> {
        let pool = ctx.data::<PgPool>()?;
        let pg_version: String = sqlx::query_scalar("select version()")
            .fetch_one(pool)
            .await
            .map_err(|err| async_graphql::Error::new(err.to_string()))?;

        Ok(format!("{} | {}", env!("CARGO_PKG_VERSION"), pg_version))
    }
}

#[derive(Default)]
pub struct MutationRoot;

#[Object]
impl MutationRoot {
    async fn health(&self, ctx: &Context<'_>) -> Result<&str> {
        let wf_engine = ctx.data::<WorkflowEngine>()?;
        wf_engine.start_health_check().await?;
        Ok("ok")
    }
}
