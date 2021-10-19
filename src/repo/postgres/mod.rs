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

// maybe this type gets removed in the future for better performance
// and we just manually map to Account
#[derive(sqlx::FromRow)]
struct PgAccount {
    id: i64,
    email: String,
}

impl From<PgAccount> for Account {
    fn from(account: PgAccount) -> Self {
        let PgAccount { id, email } = account;
        Self { id, email }
    }
}

#[async_trait]
impl<T: Send> AccountRepo for T
where
    for<'b> T: IsExecutor<'b>,
{
    type Error = sqlx::Error;

    async fn get_accounts(&mut self) -> Result<Vec<Account>, Self::Error> {
        sqlx::query_as::<_, PgAccount>("SELECT * FROM account")
            .fetch(self.coerce())
            .map_ok(Account::from)
            .try_collect()
            .await
    }

    async fn create_account(&mut self, account: &Account) -> Result<(), Self::Error> {
        sqlx::query("INSERT INTO account (id, email) VALUES ($1, $2)")
            .bind(&account.id)
            .bind(&account.email)
            .execute(self.coerce())
            .await?;

        Ok(())
    }

    async fn update_account(&mut self, account: &Account) -> Result<(), Self::Error> {
        sqlx::query("UPDATE account SET (email) VALUES ($2) WHERE id = $1")
            .bind(&account.id)
            .bind(&account.email)
            .execute(self.coerce())
            .await?;

        Ok(())
    }

    async fn delete_account(&mut self, account: Account) -> Result<(), Self::Error> {
        sqlx::query("DELETE FROM account WHERE id = $1")
            .bind(account.id)
            .execute(self.coerce())
            .await?;

        Ok(())
    }
}

async fn connect<C, S>(db: C, schema: S) -> Result<PgPool, sqlx::Error>
where
    C: AsRef<str>,
    S: Display,
{
    // SET search_path for all future queries
    // is vulnerable to SQL Injection use with caution
    // could also be done like this:
    // https://github.com/launchbadge/sqlx/issues/907#issuecomment-747072077
    // https://www.postgresql.org/docs/14/ecpg-connect.html
    // but im not sure if this would change the behavior for other db connections
    // todo: figure this out
    let schema = format!("SET search_path TO {}", schema);

    // use arc to only allocate once for all connections
    // using Arc<str> also gives us only a single misdirection
    let schema = schema.into_boxed_str().into();

    PgPoolOptions::new()
        .after_connect(move |conn| {
            let schema = Arc::clone(&schema);

            async move {
                // run prepared query
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
    use anyhow::{anyhow, Result};
    use sqlx::migrate::Migrator;
    use sqlx::{Executor, PgPool};
    use testcontainers::{clients, images, Container, Docker};

    use super::*;

    static MIGRATOR: Migrator = sqlx::migrate!();

    fn create_container(
        docker: &clients::Cli,
    ) -> Container<'_, clients::Cli, images::postgres::Postgres> {
        let postgres_image = images::postgres::Postgres::default();
        docker.run(postgres_image)
    }

    fn get_connection_string(
        container: &Container<'_, clients::Cli, images::postgres::Postgres>,
    ) -> Result<String> {
        let port = container
            .get_host_port(5432)
            .ok_or_else(|| anyhow!("Port not found"))?;

        let connection_string = format!("postgres://postgres:postgres@localhost:{}/postgres", port);
        Ok(connection_string)
    }

    async fn create_acmon_schema<T: AsRef<str>>(connection_string: T) -> Result<()> {
        let pool = PgPool::connect(connection_string.as_ref()).await?;
        // we create the schmea before running our migrations
        pool.execute("CREATE SCHEMA IF NOT EXISTS acmon;").await?;

        Ok(())
    }

    #[tokio::test]
    async fn custom_schema_works() -> Result<()> {
        let docker = clients::Cli::default();
        let container = create_container(&docker);
        let connection_string = get_connection_string(&container)?;
        create_acmon_schema(&connection_string).await?;

        let pool = connect(connection_string, "acmon").await?;
        // run migration on acmon schema
        MIGRATOR.run(&pool).await?;

        // explicitly select account on acmon schema to test if migration got run
        // on correct schema
        let count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM acmon.account")
            .fetch_one(&pool)
            .await?;
        assert_eq!(count, 0);

        // select account from different schema to make sure query fails if schema does not exist
        let failed_count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM public.account")
            .fetch_one(&pool)
            .await;
        assert!(failed_count.is_err());

        Ok(())
    }
}
