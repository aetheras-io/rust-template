use super::*;

/// Run a lightweight sanity query against the provided pool.
pub async fn run_version_check(pool: &PgPool) -> sqlx::Result<String> {
    let (version,): (String,) = sqlx::query_as("SELECT version()").fetch_one(pool).await?;
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires local postgres on localhost:5432"]
    async fn can_query_version() {
        let pool = setup_test_db("sqlx_template").await.expect("db setup");

        let version = run_version_check(&pool).await.expect("version query");
        assert!(
            version.to_lowercase().contains("postgres"),
            "unexpected version string: {version}"
        );

        teardown_test_db("sqlx_template", pool)
            .await
            .expect("db teardown");
    }
}
