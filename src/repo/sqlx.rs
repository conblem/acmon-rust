use either::Either;
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use sqlx::database::HasStatement;
use sqlx::{Database, Describe, Error, Execute, Executor};
use std::convert::Infallible;
use std::fmt::Debug;
use std::marker::PhantomData;

// this type is used in the from implementations as a placeholder for something that never exists
// it implements Executor for both &Self and &mut Self
#[derive(Debug)]
struct Never<DB> {
    inner: Infallible,
    phantom: PhantomData<DB>,
}

impl<'c, DB> Executor<'c> for &'_ Never<DB>
where
    DB: Database + Sync,
{
    type Database = DB;

    fn fetch_many<'e, 'q: 'e, E: 'q>(
        self,
        _query: E,
    ) -> BoxStream<'e, Result<Either<DB::QueryResult, DB::Row>, Error>>
    where
        'c: 'e,
        E: Execute<'q, Self::Database>,
    {
        match self.inner {}
    }

    fn fetch_optional<'e, 'q: 'e, E: 'q>(
        self,
        _query: E,
    ) -> BoxFuture<'e, Result<Option<DB::Row>, Error>>
    where
        'c: 'e,
        E: Execute<'q, Self::Database>,
    {
        match self.inner {}
    }

    fn prepare_with<'e, 'q: 'e>(
        self,
        _sql: &'q str,
        _parameters: &'e [<Self::Database as Database>::TypeInfo],
    ) -> BoxFuture<'e, Result<<Self::Database as HasStatement<'q>>::Statement, Error>>
    where
        'c: 'e,
    {
        match self.inner {}
    }

    #[doc(hidden)]
    fn describe<'e, 'q: 'e>(
        self,
        _sql: &'q str,
    ) -> BoxFuture<'e, Result<Describe<Self::Database>, Error>>
    where
        'c: 'e,
    {
        match self.inner {}
    }
}

impl<'c, DB> Executor<'c> for &'_ mut Never<DB>
where
    DB: Database + Sync,
{
    type Database = DB;

    fn fetch_many<'e, 'q: 'e, E: 'q>(
        self,
        _query: E,
    ) -> BoxStream<'e, Result<Either<DB::QueryResult, DB::Row>, Error>>
    where
        'c: 'e,
        E: Execute<'q, Self::Database>,
    {
        match self.inner {}
    }

    fn fetch_optional<'e, 'q: 'e, E: 'q>(
        self,
        _query: E,
    ) -> BoxFuture<'e, Result<Option<DB::Row>, Error>>
    where
        'c: 'e,
        E: Execute<'q, Self::Database>,
    {
        match self.inner {}
    }

    fn prepare_with<'e, 'q: 'e>(
        self,
        _sql: &'q str,
        _parameters: &'e [<Self::Database as Database>::TypeInfo],
    ) -> BoxFuture<'e, Result<<Self::Database as HasStatement<'q>>::Statement, Error>>
    where
        'c: 'e,
    {
        match self.inner {}
    }

    #[doc(hidden)]
    fn describe<'e, 'q: 'e>(
        self,
        _sql: &'q str,
    ) -> BoxFuture<'e, Result<Describe<Self::Database>, Error>>
    where
        'c: 'e,
    {
        match self.inner {}
    }
}

#[derive(Debug)]
enum Wrapper<'a, R, M> {
    Ref { inner: &'a R },
    Mut { inner: &'a mut M },
}

impl<'c, DB, R, M> Executor<'c> for Wrapper<'c, R, M>
where
    DB: Database + Sync,
    R: Debug,
    M: Debug,
    &'c R: Executor<'c, Database = DB>,
    &'c mut M: Executor<'c, Database = DB>,
{
    type Database = DB;

    fn fetch_many<'e, 'q: 'e, E: 'q>(
        self,
        query: E,
    ) -> BoxStream<'e, Result<Either<DB::QueryResult, DB::Row>, Error>>
    where
        'c: 'e,
        E: Execute<'q, Self::Database>,
    {
        match self {
            Wrapper::Ref { inner } => inner.fetch_many(query),
            Wrapper::Mut { inner } => inner.fetch_many(query),
        }
    }

    fn fetch_optional<'e, 'q: 'e, E: 'q>(
        self,
        query: E,
    ) -> BoxFuture<'e, Result<Option<DB::Row>, Error>>
    where
        'c: 'e,
        E: Execute<'q, Self::Database>,
    {
        match self {
            Wrapper::Ref { inner } => inner.fetch_optional(query),
            Wrapper::Mut { inner } => inner.fetch_optional(query),
        }
    }

    fn prepare_with<'e, 'q: 'e>(
        self,
        sql: &'q str,
        parameters: &'e [<Self::Database as Database>::TypeInfo],
    ) -> BoxFuture<'e, Result<<Self::Database as HasStatement<'q>>::Statement, Error>>
    where
        'c: 'e,
    {
        match self {
            Wrapper::Ref { inner } => inner.prepare_with(sql, parameters),
            Wrapper::Mut { inner } => inner.prepare_with(sql, parameters),
        }
    }

    #[doc(hidden)]
    fn describe<'e, 'q: 'e>(
        self,
        sql: &'q str,
    ) -> BoxFuture<'e, Result<Describe<Self::Database>, Error>>
    where
        'c: 'e,
    {
        match self {
            Wrapper::Ref { inner } => inner.describe(sql),
            Wrapper::Mut { inner } => inner.describe(sql),
        }
    }
}

// from implementation for & using Never as placeholder
impl<'a, DB, R> From<&'a R> for Wrapper<'a, R, Never<DB>> {
    fn from(inner: &'a R) -> Self {
        Wrapper::Ref { inner }
    }
}

// from implementation for &mut using Never as placeholder
impl<'a, DB, M> From<&'a mut M> for Wrapper<'a, Never<DB>, M> {
    fn from(inner: &'a mut M) -> Self {
        Wrapper::Mut { inner }
    }
}

// makes it possible to reuse the wrapper multiple times
impl<'a, 'b, R, M> From<&'b mut Wrapper<'a, R, M>> for Wrapper<'b, R, M> {
    fn from(input: &'b mut Wrapper<'a, R, M>) -> Self {
        match input {
            Wrapper::Ref { inner } => Wrapper::Ref { inner },
            Wrapper::Mut { inner } => Wrapper::Mut { inner },
        }
    }
}

#[cfg(all(test, feature = "container"))]
mod tests {
    use sqlx::postgres::PgRow;
    use sqlx::{PgPool, Postgres, Row};
    use testcontainers::{clients, images, Docker};

    use super::*;

    #[tokio::test]
    async fn test() {
        let docker = clients::Cli::default();
        let postgres_image = images::postgres::Postgres::default();
        let node = docker.run(postgres_image);

        let port = node.get_host_port(5432).unwrap();
        let connection_string = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

        let pool = PgPool::connect(&connection_string).await.unwrap();

        execute(&pool);
    }

    async fn execute<'a, R, M, I>(input: I)
    where
        R: Debug + 'a,
        M: Debug + 'a,
        for<'b> Wrapper<'b, R, M>: Executor<'b, Database = Postgres>,
        I: Into<Wrapper<'a, R, M>>,
    {
        let mut wrapper = input.into();

        let res = sqlx::query("select 1 + 1")
            .try_map(|row: PgRow| row.try_get::<i32, _>(0))
            .fetch_one(Wrapper::<'_, R, M>::from(&mut wrapper))
            .await
            .unwrap();
        assert_eq!(2, res);

        let res = sqlx::query("select 2 + 2")
            .try_map(|row: PgRow| row.try_get::<i32, _>(0))
            .fetch_one(Wrapper::<'_, R, M>::from(&mut wrapper))
            .await
            .unwrap();
        assert_eq!(4, res);
    }
}
