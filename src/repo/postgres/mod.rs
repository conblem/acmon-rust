use async_trait::async_trait;
use futures_util::FutureExt;
use sea_query::{Expr, Iden, PostgresQueryBuilder, Query, Value};
use sqlx::migrate::Migrator;
use sqlx::postgres::{PgPoolOptions, PgRow};
use sqlx::{FromRow, PgPool};
use std::fmt::Display;
use std::sync::Arc;

sea_query::sea_query_driver_postgres!();
use sea_query_driver_postgres::{bind_query, bind_query_as};

use super::account::{AccountStruct, Repo};
use super::executor::IsExecutor;

static MIGRATOR: Migrator = sqlx::migrate!();

pub(super) trait Entity: Send + Sync + Unpin + 'static
where
    for<'r> Self: FromRow<'r, PgRow>,
{
    type Iden: Iden + Copy + Send + Sync + 'static;

    fn table_iden() -> Self::Iden;
    fn id_iden() -> Self::Iden;
    fn id(&self) -> i32;
    fn id_mut(&mut self) -> &mut i32;
    fn columns_iden() -> Vec<Self::Iden>;
    fn values(&self) -> Vec<(Self::Iden, Value)>;
}

#[derive(Iden, Clone, Copy)]
pub(super) enum Account {
    Table,
    Id,
    Email,
}

impl Entity for AccountStruct {
    type Iden = Account;

    fn table_iden() -> Self::Iden {
        Account::Table
    }

    fn id_iden() -> Self::Iden {
        Account::Id
    }

    fn id(&self) -> i32 {
        self.id
    }

    fn id_mut(&mut self) -> &mut i32 {
        &mut self.id
    }

    fn columns_iden() -> Vec<Self::Iden> {
        vec![Account::Email]
    }

    fn values(&self) -> Vec<(Self::Iden, Value)> {
        vec![(Account::Email, self.email.clone().into())]
    }
}

#[async_trait]
impl<E: Entity, T: Send> Repo<E> for T
where
    for<'b> T: IsExecutor<'b>,
{
    type Error = sqlx::Error;

    // todo: write test for this
    async fn read(&mut self, id: i32) -> Result<Option<E>, Self::Error> {
        let table = E::table_iden();
        let id_iden = E::id_iden();
        let columns_iden = E::columns_iden();

        let (sql, values) = Query::select()
            .column(id_iden)
            .columns(columns_iden)
            .from(table)
            .and_where(Expr::col(id_iden).eq(id))
            .limit(1)
            .build(PostgresQueryBuilder);

        bind_query_as(sqlx::query_as(&sql), &values)
            .fetch_optional(self.coerce())
            .await
    }

    async fn read_all(&mut self) -> Result<Vec<E>, Self::Error> {
        let table = E::table_iden();
        let id_iden = E::id_iden();
        let columns_iden = E::columns_iden();

        let (sql, values) = Query::select()
            .column(id_iden)
            .columns(columns_iden)
            .from(table)
            .build(PostgresQueryBuilder);

        bind_query_as(sqlx::query_as(&sql), &values)
            .fetch_all(self.coerce())
            .await
    }

    async fn create(&mut self, account: &mut E) -> Result<(), Self::Error> {
        let table = E::table_iden();
        let id_iden = E::id_iden();
        let values = account.values();

        let (columns, values): (Vec<E::Iden>, Vec<Value>) = values.into_iter().unzip();
        let (sql, values) = Query::insert()
            .into_table(table)
            .columns(columns)
            .values_panic(values)
            .returning_col(id_iden)
            .build(PostgresQueryBuilder);

        let (id,) = bind_query_as(sqlx::query_as(&sql), &values)
            .fetch_one(self.coerce())
            .await?;

        *account.id_mut() = id;

        Ok(())
    }

    async fn update(&mut self, account: &E) -> Result<(), Self::Error> {
        let table = E::table_iden();
        let id_iden = E::id_iden();
        let values = account.values();

        let (sql, values) = Query::update()
            .table(table)
            .values(values)
            .and_where(Expr::col(id_iden).eq(account.id()))
            .build(PostgresQueryBuilder);

        bind_query(sqlx::query(&sql), &values)
            .execute(self.coerce())
            .await?;

        Ok(())
    }

    async fn delete(&mut self, account: E) -> Result<(), Self::Error> {
        let table = E::table_iden();
        let id_iden = E::id_iden();

        let (sql, values) = Query::delete()
            .from_table(table)
            .and_where(Expr::col(id_iden).eq(account.id()))
            .build(PostgresQueryBuilder);

        bind_query(sqlx::query(&sql), &values)
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
    async fn fetch_accounts_works() -> Result<()> {
        let docker = clients::Cli::default();
        let (_container, connection_string) = create_mock_database(&docker).await?;

        let pool = connect_and_run_migration(connection_string, "acmon").await?;

        let accounts: Vec<AccountStruct> = (&pool).read_all().await?;
        assert_eq!(accounts.len(), 0);

        // insert example row for test
        (&pool)
            .execute("INSERT INTO account (email) VALUES ('test@test.com')")
            .await?;

        let accounts: Vec<AccountStruct> = (&pool).read_all().await?;
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
        let mut expected = AccountStruct {
            id: 0,
            email: "test@test.com".to_string(),
        };
        (&pool).create(&mut expected).await?;
        // id should be set now
        assert_eq!(expected.id, 1);

        // we get the accounts using the pool method which is tested
        // that's why we can now use it in a test
        let accounts: Vec<AccountStruct> = (&pool).read_all().await?;
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

        let mut account = AccountStruct {
            id: 0,
            email: "test@test.com".to_string(),
        };
        // create account is also tested now so we use it in this test
        (&pool).create(&mut account).await?;
        (&pool).delete(account).await?;

        // ditto
        let accounts: Vec<AccountStruct> = (&pool).read_all().await?;
        assert_eq!(accounts.len(), 0);

        Ok(())
    }
}
