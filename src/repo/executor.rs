use sea_query::{PostgresQueryBuilder, QueryBuilder, Values};
use sqlx::database::HasArguments;
use sqlx::{Database, Executor, Postgres};

pub(super) trait IsExecutor<'b, DB> {
    type Bound: Executor<'b, Database = DB>;
    fn coerce(&'b mut self) -> Self::Bound;
}

impl<'a, 'b, R: 'b, DB> IsExecutor<'b, DB> for &'a R
where
    &'b R: Executor<'b, Database = DB>,
{
    type Bound = &'b R;
    fn coerce(&'b mut self) -> Self::Bound {
        self
    }
}

impl<'a, 'b, M: 'b, DB> IsExecutor<'b, DB> for &'a mut M
where
    &'b mut M: Executor<'b, Database = DB>,
{
    type Bound = &'b mut M;
    fn coerce(&'b mut self) -> Self::Bound {
        self
    }
}

type Query<'a, DB> = sqlx::query::Query<'a, DB, <DB as HasArguments<'a>>::Arguments>;
type QueryAs<'a, DB, T> = sqlx::query::QueryAs<'a, DB, T, <DB as HasArguments<'a>>::Arguments>;

pub(super) trait IsQueryBuilder: Database {
    type QueryBuilder: QueryBuilder;
    fn query_builder() -> Self::QueryBuilder;

    fn bind_query<'a>(sql: &'a str, params: &'a Values) -> Query<'a, Self>;
    fn bind_query_as<'a, T>(
        query: QueryAs<'a, Self, T>,
        params: &'a Values,
    ) -> QueryAs<'a, Self, T>;
}

sea_query::sea_query_driver_postgres!();

impl IsQueryBuilder for Postgres {
    type QueryBuilder = PostgresQueryBuilder;

    fn query_builder() -> Self::QueryBuilder {
        PostgresQueryBuilder
    }

    fn bind_query<'a>(sql: &'a str, params: &'a Values) -> Query<'a, Self> {
        let query = sqlx::query(sql);
        sea_query_driver_postgres::bind_query(query, params)
    }

    fn bind_query_as<'a, T>(
        query: QueryAs<'a, Self, T>,
        params: &'a Values,
    ) -> QueryAs<'a, Self, T> {
        sea_query_driver_postgres::bind_query_as(query, params)
    }
}

#[cfg(all(test, feature = "container"))]
mod tests {
    use sqlx::{ColumnIndex, Database, IntoArguments, PgPool, Row, Type};
    use testcontainers::{clients, images, Docker};

    use super::*;
    use sqlx::decode::Decode;

    #[tokio::test]
    async fn test() {
        let docker = clients::Cli::default();
        let postgres_image = images::postgres::Postgres::default();
        let node = docker.run(postgres_image);

        let port = node.get_host_port(5432).unwrap();
        let connection_string = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

        let pool = PgPool::connect(&connection_string).await.unwrap();
        execute(&pool).await;
        execute(&pool).await;

        let mut conn = pool.acquire().await.unwrap();
        execute(&mut conn).await;
        execute(&mut conn).await;

        let mut transaction = pool.begin().await.unwrap();
        execute(&mut transaction).await;
        execute(&mut transaction).await;
    }

    async fn execute<T, DB: Database>(mut executor: T)
    where
        for<'b> T: IsExecutor<'b, DB>,
        for<'b> <DB as HasArguments<'static>>::Arguments: IntoArguments<'b, DB>,
        for<'b> i32: Decode<'b, DB>,
        i32: Type<DB>,
        usize: ColumnIndex<DB::Row>,
    {
        let res = sqlx::query("select 1 + 1")
            .try_map(|row: DB::Row| row.try_get::<i32, _>(0))
            .fetch_one(executor.coerce())
            .await
            .unwrap();
        assert_eq!(2, res);

        let res = sqlx::query("select 2 + 2")
            .try_map(|row: DB::Row| row.try_get::<i32, _>(0))
            .fetch_one(executor.coerce())
            .await
            .unwrap();
        assert_eq!(4, res);
    }
}
