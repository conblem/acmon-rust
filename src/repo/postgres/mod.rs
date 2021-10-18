use async_trait::async_trait;
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};

use super::account::AccountRepo;
use super::executor::IsExecutor;

static MIGRATOR: Migrator = sqlx::migrate!();

struct PostgresAccountRepo<T> {
    inner: T,
}

impl<T> From<T> for PostgresAccountRepo<T> {
    fn from(inner: T) -> Self {
        Self { inner }
    }
}

struct Test {
    id: i64,
}

#[derive(sqlx::FromRow)]
struct PostgresAccount {
    id: i64,
}

impl From<PostgresAccount> for Test {
    fn from(account: PostgresAccount) -> Self {
        let PostgresAccount { id } = account;
        Self { id }
    }
}

#[async_trait]
impl<T: Send> AccountRepo for PostgresAccountRepo<T>
where
    for<'b> T: IsExecutor<'b>,
{
    async fn get_account(&mut self, _input: &str) {
        let res = sqlx::query_as::<_, PostgresAccount>("SELECT * FROM ACCOUNT")
            .fetch_all(self.inner.coerce())
            .await
            .unwrap();
    }
}

async fn connect<C, S>(db: C, schema: S) -> Result<PgPool, sqlx::Error>
where
    C: AsRef<str>,
    S: Into<String>,
{
    let schema = schema.into();

    PgPoolOptions::new()
        .after_connect(move |conn| {
            let schema = schema.clone();

            Box::pin(async move {
                // always use this schema for connections
                sqlx::query("SET search_path TO $1")
                    .bind(schema)
                    .execute(conn);

                Ok(())
            })
        })
        .connect(db.as_ref())
        .await
}

#[cfg(all(test, feature = "container"))]
mod tests {
    use anyhow::Result;
    use sqlx::migrate::Migrator;
    use sqlx::PgPool;
    use testcontainers::{clients, images, Docker};

    static MIGRATOR: Migrator = sqlx::migrate!();

    use super::*;

    #[tokio::test]
    async fn test() -> Result<()> {
        let docker = clients::Cli::default();
        let postgres_image = images::postgres::Postgres::default();
        let node = docker.run(postgres_image);

        let port = node.get_host_port(5432).unwrap();
        let connection_string = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

        // create schema acmon needs the schema acmon
        let pool = PgPool::connect(&connection_string).await?;
        pool.execute("CREATE SCHEMA IF NOT EXISTS acmon;").await?;

        let schema = "acmon".to_string();
        let pool = connect(&connection_string, schema).await?;
        MIGRATOR.run(&pool).await?;

        let mut account_repo = PostgresAccountRepo::from(&pool);
        account_repo.get_account("test").await;

        Ok(())
    }
}
