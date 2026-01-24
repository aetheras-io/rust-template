use super::{PgPool, SqlxError, ensure_affected};
use sqlx::{Executor, Postgres};

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

pub trait PgExecutor<'c>: Executor<'c, Database = Postgres> {}
impl<'c, T: Executor<'c, Database = Postgres>> PgExecutor<'c> for T {}

pub async fn create_user(pool: &PgPool, email: &str) -> Result<User, SqlxError> {
    create_user_with_executor(pool, email).await
}

pub async fn get_user_by_id(pool: &PgPool, id: sqlx::types::Uuid) -> Result<User, SqlxError> {
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

pub async fn get_user_by_email(pool: &PgPool, email: &str) -> Result<User, SqlxError> {
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
) -> Result<User, SqlxError> {
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

pub async fn delete_user(pool: &PgPool, id: sqlx::types::Uuid) -> Result<(), SqlxError> {
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
) -> Result<UserMfaMethod, SqlxError> {
    add_mfa_method_with_executor(pool, user_id, kind, secret).await
}

pub async fn list_mfa_methods(
    pool: &PgPool,
    user_id: sqlx::types::Uuid,
) -> Result<Vec<UserMfaMethod>, SqlxError> {
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

pub async fn delete_mfa_method(
    pool: &PgPool,
    mfa_id: sqlx::types::Uuid,
) -> Result<(), SqlxError> {
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
) -> Result<(User, UserMfaMethod), SqlxError> {
    let mut tx = pool.begin().await?;
    let user = create_user_with_executor(tx.as_mut(), email).await?;
    let mfa = add_mfa_method_with_executor(tx.as_mut(), user.id, kind, secret).await?;
    tx.commit().await?;
    Ok((user, mfa))
}

pub async fn create_user_with_executor<'c, T: PgExecutor<'c>>(
    executor: T,
    email: &str,
) -> Result<User, SqlxError> {
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
) -> Result<UserMfaMethod, SqlxError> {
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
