use super::ensure_affected;
use sea_query::{Expr, ExprTrait, Iden, Order, PostgresQueryBuilder, Query, SelectStatement};
use sea_query_sqlx::SqlxBinder;
use sqlx::{Error, PgPool};
use sqlx::{Executor, Postgres};

const DEFAULT_USER_PAGE_SIZE: u32 = 25;
const MAX_USER_PAGE_SIZE: u32 = 100;

#[derive(Iden)]
#[iden = "user_mfa_methods"]
enum UserMfaMethods {
    Table,
    Id,
    UserId,
    Kind,
    Secret,
    CreatedAt,
    UpdatedAt,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: sqlx::types::Uuid,
    pub email: String,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserMfaMethod {
    pub id: sqlx::types::Uuid,
    pub user_id: sqlx::types::Uuid,
    pub kind: String,
    pub secret: String,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
}

/// A stable keyset cursor for a user's MFA methods ordered newest-first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UserMfaCursor {
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub id: sqlx::types::Uuid,
}

#[derive(Debug, Clone)]
pub struct UserMfaPage {
    pub methods: Vec<UserMfaMethod>,
    pub next_cursor: Option<UserMfaCursor>,
}

pub trait PgExecutor<'c>: Executor<'c, Database = Postgres> {}
impl<'c, T: Executor<'c, Database = Postgres>> PgExecutor<'c> for T {}

pub async fn create_user(pool: &PgPool, email: &str) -> Result<User, Error> {
    create_user_with_executor(pool, email).await
}

pub async fn get_user_by_id(pool: &PgPool, id: sqlx::types::Uuid) -> Result<User, Error> {
    sqlx::query_as(
        r#"
        SELECT id, email, created_at, updated_at
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
}

pub async fn get_user_by_email(pool: &PgPool, email: &str) -> Result<User, Error> {
    sqlx::query_as(
        r#"
        SELECT id, email, created_at, updated_at
        FROM users
        WHERE email = $1
        "#,
    )
    .bind(email)
    .fetch_one(pool)
    .await
}

pub async fn update_user_email(
    pool: &PgPool,
    id: sqlx::types::Uuid,
    email: &str,
) -> Result<User, Error> {
    sqlx::query_as(
        r#"
        UPDATE users
        SET email = $1, updated_at = now()
        WHERE id = $2
        RETURNING id, email, created_at, updated_at
        "#,
    )
    .bind(email)
    .bind(id)
    .fetch_one(pool)
    .await
}

pub async fn delete_user(pool: &PgPool, id: sqlx::types::Uuid) -> Result<(), Error> {
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .and_then(ensure_affected(1))
}

pub async fn add_mfa_method(
    pool: &PgPool,
    user_id: sqlx::types::Uuid,
    kind: &str,
    secret: &str,
) -> Result<UserMfaMethod, Error> {
    add_mfa_method_with_executor(pool, user_id, kind, secret).await
}

pub async fn list_mfa_methods(
    pool: &PgPool,
    user_id: sqlx::types::Uuid,
) -> Result<Vec<UserMfaMethod>, Error> {
    sqlx::query_as(
        r#"
        SELECT id, user_id, kind, secret, created_at, updated_at
        FROM user_mfa_methods
        WHERE user_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// List only the authenticated user's MFA methods with keyset pagination.
///
/// SeaQuery owns the dynamic SQL structure while the user scope, cursor, and
/// limit stay as SQLx bind parameters. The `AssertSqlSafe` call is the audited
/// handoff between those two libraries; it does not perform sanitization itself.
pub async fn list_mfa_methods_page(
    pool: &PgPool,
    user_id: sqlx::types::Uuid,
    after: Option<UserMfaCursor>,
    page_size: Option<u32>,
) -> Result<UserMfaPage, Error> {
    let page_size = page_size
        .unwrap_or(DEFAULT_USER_PAGE_SIZE)
        .clamp(1, MAX_USER_PAGE_SIZE);
    let statement = mfa_methods_page_statement(user_id, after, u64::from(page_size) + 1);
    let (sql, values) = statement.build_sqlx(PostgresQueryBuilder);

    let mut methods: Vec<UserMfaMethod> = sqlx::query_as_with(sqlx::AssertSqlSafe(sql), values)
        .fetch_all(pool)
        .await?;
    let has_next_page = methods.len() > page_size as usize;
    methods.truncate(page_size as usize);
    let next_cursor = has_next_page
        .then(|| methods.last().map(UserMfaCursor::from))
        .flatten();

    Ok(UserMfaPage {
        methods,
        next_cursor,
    })
}

fn mfa_methods_page_statement(
    user_id: sqlx::types::Uuid,
    after: Option<UserMfaCursor>,
    fetch_limit: u64,
) -> SelectStatement {
    let mut statement = Query::select();
    statement
        .columns([
            UserMfaMethods::Id,
            UserMfaMethods::UserId,
            UserMfaMethods::Kind,
            UserMfaMethods::Secret,
            UserMfaMethods::CreatedAt,
            UserMfaMethods::UpdatedAt,
        ])
        .from(UserMfaMethods::Table)
        .and_where(Expr::col(UserMfaMethods::UserId).eq(user_id))
        .order_by(UserMfaMethods::CreatedAt, Order::Desc)
        .order_by(UserMfaMethods::Id, Order::Desc)
        .limit(fetch_limit);

    if let Some(after) = after {
        statement.cond_where(
            Expr::tuple([
                Expr::col(UserMfaMethods::CreatedAt),
                Expr::col(UserMfaMethods::Id),
            ])
            .lt(Expr::tuple([
                Expr::value(after.created_at),
                Expr::value(after.id),
            ])),
        );
    }

    statement.to_owned()
}

impl From<&UserMfaMethod> for UserMfaCursor {
    fn from(method: &UserMfaMethod) -> Self {
        Self {
            created_at: method.created_at,
            id: method.id,
        }
    }
}

pub async fn delete_mfa_method(pool: &PgPool, mfa_id: sqlx::types::Uuid) -> Result<(), Error> {
    sqlx::query("DELETE FROM user_mfa_methods WHERE id = $1")
        .bind(mfa_id)
        .execute(pool)
        .await
        .and_then(ensure_affected(1))
}

pub async fn create_user_with_mfa(
    pool: &PgPool,
    email: &str,
    kind: &str,
    secret: &str,
) -> Result<(User, UserMfaMethod), Error> {
    let mut tx = pool.begin().await?;
    let user = create_user_with_executor(tx.as_mut(), email).await?;
    let mfa = add_mfa_method_with_executor(tx.as_mut(), user.id, kind, secret).await?;
    tx.commit().await?;
    Ok((user, mfa))
}

pub async fn create_user_with_executor<'c, T: PgExecutor<'c>>(
    executor: T,
    email: &str,
) -> Result<User, Error> {
    sqlx::query_as(
        r#"
        INSERT INTO users (email)
        VALUES ($1)
        RETURNING id, email, created_at, updated_at
        "#,
    )
    .bind(email)
    .fetch_one(executor)
    .await
}

pub async fn add_mfa_method_with_executor<'c, T: PgExecutor<'c>>(
    executor: T,
    user_id: sqlx::types::Uuid,
    kind: &str,
    secret: &str,
) -> Result<UserMfaMethod, Error> {
    sqlx::query_as(
        r#"
        INSERT INTO user_mfa_methods (user_id, kind, secret)
        VALUES ($1, $2, $3)
        RETURNING id, user_id, kind, secret, created_at, updated_at
        "#,
    )
    .bind(user_id)
    .bind(kind)
    .bind(secret)
    .fetch_one(executor)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mfa_page_query_binds_user_scope_and_keyset_cursor() {
        let user_id = sqlx::types::Uuid::from_u128(1);
        let cursor = UserMfaCursor {
            created_at: "2026-01-24T12:00:00Z".parse().expect("valid timestamp"),
            id: sqlx::types::Uuid::nil(),
        };

        let (sql, values) =
            mfa_methods_page_statement(user_id, Some(cursor), 26).build(PostgresQueryBuilder);

        assert!(sql.contains(r#"WHERE "user_id" = $1"#));
        assert!(sql.contains(r#"("created_at", "id") < ($2, $3)"#));
        assert!(sql.contains(r#"ORDER BY "created_at" DESC, "id" DESC"#));
        assert!(sql.ends_with("LIMIT $4"), "{sql}");
        assert_eq!(values.0.len(), 4, "{sql}");
    }
}
