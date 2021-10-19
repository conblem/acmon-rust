use async_trait::async_trait;
use futures_util::{FutureExt, TryStreamExt};
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::fmt::Display;
use std::sync::Arc;

use super::account::{Account, AccountRepo};
use super::executor::IsExecutor;

static MIGRATOR: Migrator = sqlx::migrate!();

#[derive(sqlx::FromRow)]
struct PgAccount {
    id: i64,
}

impl From<PgAccount> for Account {
    fn from(account: PgAccount) -> Self {
        let PgAccount { id } = account;
        Self { id }
    }
}

#[async_trait]
impl<T: Send> AccountRepo for T
where
    for<'b> T: IsExecutor<'b>,
{
    async fn get_accounts(&mut self) -> Vec<Account> {
        sqlx::query_as::<_, PgAccount>("SELECT * FROM account")
            .fetch(self.coerce())
            .map_ok(Account::from)
            .try_collect()
            .await
            .unwrap()
    }
}

async fn connect<C, S>(db: C, schema: S) -> Result<PgPool, sqlx::Error>
where
    C: AsRef<str>,
    S: Display,
{
    let schema = format!("SET search_path TO {}", schema);
    // use arc to only allocate once for all connections
    let schema = schema.into_boxed_str().into();

    PgPoolOptions::new()
        .after_connect(move |conn| {
            let schema = Arc::clone(&schema);

            async move {
                // always use this schema for connections
                sqlx::query(&schema).execute(conn).await?;

                Ok(())
            }
            .boxed()
        })
        .connect(db.as_ref())
        .await
}

#[cfg(all(test, feature = "container"))]
mod tests {
    use anyhow::Result;
    use sqlx::migrate::Migrator;
    use sqlx::{Executor, PgPool};
    use testcontainers::{clients, images, Docker};

    use super::*;

    static MIGRATOR: Migrator = sqlx::migrate!();

    #[tokio::test]
    async fn test() -> Result<()> {
        let docker = clients::Cli::default();
        let postgres_image = images::postgres::Postgres::default();
        let node = docker.run(postgres_image);

        let port = node.get_host_port(5432).unwrap();
        let connection_string = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

        // create schema acmon
        // connection fails otherwise
        // todo: create a test for this
        let pool = PgPool::connect(&connection_string).await?;
        pool.execute("CREATE SCHEMA IF NOT EXISTS acmon;").await?;

        let pool = connect(connection_string, "acmon").await?;
        MIGRATOR.run(&pool).await?;

        (&pool).get_accounts().await;

        Ok(())
    }

    #[tokio::test]
    async fn should_fail_if_schema_does_not_exist() {}
}
