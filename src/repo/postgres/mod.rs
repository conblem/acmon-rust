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
    id: i32,
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

    // todo: write test for this
    async fn fetch_account(&mut self, id: i32) -> Result<Option<Account>, Self::Error> {
        let account = sqlx::query_as::<_, PgAccount>("SELECT * FROM account where id = $1 LIMIT 1")
            .bind(id)
            .fetch_optional(self.coerce())
            .await?;

        Ok(account.map(Into::into))
    }

    async fn fetch_accounts(&mut self) -> Result<Vec<Account>, Self::Error> {
        sqlx::query_as::<_, PgAccount>("SELECT * FROM account")
            .fetch(self.coerce())
            .map_ok(Account::from)
            .try_collect()
            .await
    }

    async fn create_account(&mut self, account: &mut Account) -> Result<(), Self::Error> {
        // we return the id of the created account to set on the param
        let id = sqlx::query_scalar("INSERT INTO account (email) VALUES ($1) RETURNING id")
            .bind(&account.email)
            .fetch_one(self.coerce())
            .await?;

        account.id = id;

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

async fn connect_and_run_migration<C, S>(db: C, schema: S) -> Result<PgPool, sqlx::Error>
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

    let pool = PgPoolOptions::new()
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
        .await?;

    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

#[cfg(all(test, feature = "container"))]
mod tests {
    use anyhow::{anyhow, Result};
    use sqlx::{Executor, PgPool};
    use testcontainers::{clients, images, Container, Docker};

    use super::*;

    async fn create_mock_database(
        docker: &clients::Cli,
    ) -> Result<(
        Container<'_, clients::Cli, images::postgres::Postgres>,
        String,
    )> {
        let postgres_image = images::postgres::Postgres::default();
        let container = docker.run(postgres_image);

        let port = container
            .get_host_port(5432)
            .ok_or_else(|| anyhow!("Port not found"))?;

        let connection_string = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

        let pool = PgPool::connect(connection_string.as_ref()).await?;
        // we create the schema before running our migrations
        pool.execute("CREATE SCHEMA IF NOT EXISTS acmon;").await?;

        Ok((container, connection_string))
    }

    #[tokio::test]
    async fn custom_schema_works() -> Result<()> {
        let docker = clients::Cli::default();
        let (_container, connection_string) = create_mock_database(&docker).await?;

        let pool = connect_and_run_migration(connection_string, "acmon").await?;

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

    #[tokio::test]
    async fn get_accounts_works() -> Result<()> {
        let docker = clients::Cli::default();
        let (_container, connection_string) = create_mock_database(&docker).await?;

        let pool = connect_and_run_migration(connection_string, "acmon").await?;

        let accounts = (&pool).fetch_accounts().await?;
        assert_eq!(accounts.len(), 0);

        // insert example row for test
        (&pool)
            .execute("INSERT INTO account (email) VALUES ('test@test.com')")
            .await?;

        let accounts = (&pool).fetch_accounts().await?;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].email, "test@test.com");

        Ok(())
    }

    #[tokio::test]
    async fn create_account_works() -> Result<()> {
        let docker = clients::Cli::default();
        let (_container, connection_string) = create_mock_database(&docker).await?;

        let pool = connect_and_run_migration(connection_string, "acmon").await?;

        // insert example row for test
        let mut expected = Account {
            id: 0,
            email: "test@test.com".to_string(),
        };
        (&pool).create_account(&mut expected).await?;
        // id should be set now
        assert_eq!(expected.id, 1);

        // we get the accounts using the pool method which is tested
        // that's why we can now use it in a test
        let accounts = (&pool).fetch_accounts().await?;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, 1);
        assert_eq!(accounts[0].email, "test@test.com");

        Ok(())
    }

    #[tokio::test]
    async fn delete_account_works() -> Result<()> {
        let docker = clients::Cli::default();
        let (_container, connection_string) = create_mock_database(&docker).await?;

        let pool = connect_and_run_migration(connection_string, "acmon").await?;

        let mut account = Account {
            id: 0,
            email: "test@test.com".to_string(),
        };
        // create account is also tested now so we use it in this test
        (&pool).create_account(&mut account).await?;
        (&pool).delete_account(account).await?;

        // ditto
        let accounts = (&pool).fetch_accounts().await?;
        assert_eq!(accounts.len(), 0);

        Ok(())
    }
}
