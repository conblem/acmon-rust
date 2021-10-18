use async_trait::async_trait;
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool, Acquire};

use super::account::AccountRepo;
use super::executor::IsExecutor;
use crate::config::Config;

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
    id: i64
}

#[derive(sqlx::FromRow)]
struct PostgresAccount {
    id: i64,
}

impl From<PostgresAccount> for Test {
    fn from(account: PostgresAccount) -> Self {
        let PostgresAccount { id } = account;
        Self {
            id
        }
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

async fn connect(db: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .after_connect(|conn| {
            Box::pin(async move {
                let mut transaction = conn.begin().await?;

                // always use this schema for connections
                transaction.execute("CREATE SCHEMA IF NOT EXISTS acmon;").await?;
                transaction.execute("SET search_path TO acmon;").await?;
                transaction.commit().await?;

                Ok(())
            })
        })
        .connect(db)
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
        let pool = connect(&connection_string).await?;
        MIGRATOR.run(&pool).await?;

        let mut account_repo = PostgresAccountRepo::from(&pool);
        account_repo.get_account("test").await;

        Ok(())
    }
}
