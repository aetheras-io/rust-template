use async_graphql::{
    Context, EmptySubscription, Object, Result, Schema, SchemaBuilder, SimpleObject,
    connection::{Connection, CursorType, Edge, OpaqueCursor},
};

use {{ project-name | snake_case }}_core::sqlx_postgres::users::{self, UserMfaCursor};
use {{ project-name | snake_case }}_core::temporal::WorkflowEngine;
use sqlx::PgPool;

const DEFAULT_MFA_PAGE_SIZE: i32 = 25;
const MAX_MFA_PAGE_SIZE: i32 = 100;

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

    /// A forward-only Relay connection backed by SeaQuery keyset pagination.
    async fn mfa_methods(
        &self,
        ctx: &Context<'_>,
        after: Option<String>,
        first: Option<i32>,
    ) -> Result<Connection<OpaqueCursor<UserMfaCursor>, UserMfaMethodNode>> {
        let first = first.unwrap_or(DEFAULT_MFA_PAGE_SIZE);
        if !(1..=MAX_MFA_PAGE_SIZE).contains(&first) {
            return Err(format!("first must be between 1 and {MAX_MFA_PAGE_SIZE}").into());
        }

        let after = after
            .as_deref()
            .map(OpaqueCursor::<UserMfaCursor>::decode_cursor)
            .transpose()
            .map_err(|error| async_graphql::Error::new(format!("invalid cursor: {error}")))?;
        let has_previous_page = after.is_some();
        let user_id = *ctx.data::<sqlx::types::Uuid>()?;
        let pool = ctx.data::<PgPool>()?;
        let page = users::list_mfa_methods_page(
            pool,
            user_id,
            after.map(|cursor| cursor.0),
            Some(first as u32),
        )
        .await
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;

        let mut connection = Connection::new(has_previous_page, page.next_cursor.is_some());
        connection
            .edges
            .extend(page.methods.into_iter().map(|method| {
                let cursor = UserMfaCursor::from(&method);
                Edge::new(OpaqueCursor(cursor), UserMfaMethodNode::from(method))
            }));
        Ok(connection)
    }
}

#[derive(SimpleObject)]
struct UserMfaMethodNode {
    id: sqlx::types::Uuid,
    kind: String,
    created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    updated_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
}

impl From<{{ project-name | snake_case }}_core::sqlx_postgres::users::UserMfaMethod> for UserMfaMethodNode {
    fn from(method: {{ project-name | snake_case }}_core::sqlx_postgres::users::UserMfaMethod) -> Self {
        Self {
            id: method.id,
            kind: method.kind,
            created_at: method.created_at,
            updated_at: method.updated_at,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mfa_cursor_is_opaque_and_round_trips() {
        let cursor = UserMfaCursor {
            created_at: "2026-01-24T12:00:00Z".parse().expect("valid timestamp"),
            id: sqlx::types::Uuid::nil(),
        };

        let encoded = OpaqueCursor(cursor).encode_cursor();
        assert!(!encoded.contains("2026-01-24"));
        assert_eq!(
            OpaqueCursor::<UserMfaCursor>::decode_cursor(&encoded)
                .expect("cursor round trip")
                .0,
            cursor
        );
    }

    #[test]
    fn schema_exposes_the_scoped_mfa_connection() {
        let sdl = schema().finish().sdl();
        assert!(sdl.contains("mfaMethods(after: String, first: Int)"));
        assert!(sdl.contains("type UserMfaMethodNodeConnection"));
        assert!(!sdl.contains("email: String!"));
        assert!(!sdl.contains("secret: String!"));
    }

    #[tokio::test]
    async fn mfa_connection_rejects_invalid_page_arguments_before_querying() {
        let schema = schema().finish();
        for query in [
            "{ mfaMethods(first: 0) { pageInfo { hasNextPage } } }",
            "{ mfaMethods(first: 101) { pageInfo { hasNextPage } } }",
            r#"{ mfaMethods(after: "not-a-cursor") { pageInfo { hasNextPage } } }"#,
        ] {
            let response = schema.execute(query).await;
            assert!(
                !response.errors.is_empty(),
                "query unexpectedly passed: {query}"
            );
        }
    }
}
