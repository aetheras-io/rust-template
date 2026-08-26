use crate::{
    graphql::{MutationRoot, QueryRoot},
    opts::{Decoder, Encoder},
};

use async_graphql::{EmptySubscription, Schema};
use axum::extract::FromRef;
use {{ project-name | snake_case }}_core::temporal::WorkflowEngine;
use sqlx::PgPool;

#[derive(Clone, FromRef)]
pub struct AppState {
    pub schema: Schema<QueryRoot, MutationRoot, EmptySubscription>,
    pub wf_engine: WorkflowEngine,
    pub pg_pool: PgPool,
    pub jwt_encoder: Encoder,
    pub jwt_decoder: Decoder,
}

impl AppState {
    pub fn new(
        schema: Schema<QueryRoot, MutationRoot, EmptySubscription>,
        wf_engine: WorkflowEngine,
        pg_pool: PgPool,
        jwt_encoder: Encoder,
        jwt_decoder: Decoder,
    ) -> Self {
        Self {
            schema,
            wf_engine,
            pg_pool,
            jwt_encoder,
            jwt_decoder,
        }
    }
}
