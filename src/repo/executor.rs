use either::Either;
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use sqlx::database::HasStatement;
use sqlx::{Acquire, Database, Describe, Error, Execute, Executor, Transaction};
use std::convert::Infallible;
use std::fmt::Debug;
use std::marker::{PhantomData, PhantomPinned};
use std::ops::DerefMut;

#[derive(Debug)]
struct Never<DB> {
    inner: Infallible,
    phantom: PhantomData<DB>,
}

impl<DB> Unpin for Never<DB> {}

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
enum Wrapper<'a, R, M>
where
    R: Unpin,
    M: Unpin,
{
    Ref {
        inner: &'a R,
        _phantom: PhantomPinned,
    },
    Mut {
        inner: &'a mut M,
    },
}

impl<'c, DB, R, M> Executor<'c> for &'c mut Wrapper<'c, R, M>
where
    DB: Database + Sync,
    R: Unpin + Debug,
    M: Unpin + Debug,
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
            Wrapper::Ref { inner, _phantom } => inner.fetch_many(query),
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
            Wrapper::Ref { inner, _phantom } => inner.fetch_optional(query),
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
            Wrapper::Ref { inner, _phantom } => inner.prepare_with(sql, parameters),
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
            Wrapper::Ref { inner, _phantom } => inner.describe(sql),
            Wrapper::Mut { inner } => inner.describe(sql),
        }
    }
}

impl<'a, DB, R: Unpin> From<&'a R> for Wrapper<'a, R, Never<DB>>
where
    &'a R: Executor<'a, Database = DB>,
{
    fn from(inner: &'a R) -> Self {
        Wrapper::Ref {
            inner,
            _phantom: PhantomPinned,
        }
    }
}

impl<'a, DB, M: Unpin> From<&'a mut M> for Wrapper<'a, Never<DB>, M>
where
    &'a mut M: Executor<'a, Database = DB>,
{
    fn from(inner: &'a mut M) -> Self {
        Wrapper::Mut { inner }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::database::HasArguments;
    use sqlx::decode::Decode;
    use sqlx::{ColumnIndex, IntoArguments, PgPool, Row, Type};

    use super::*;

    #[tokio::test]
    async fn test() {
        let pool = PgPool::connect("hallo").await.unwrap();
        convert(&pool).await;

        //let mut conn = pool.acquire().await.unwrap();
        //let res = convert(&mut conn).await;
        //println!("{:?}", res);
    }

    async fn convert<'a, DB, R, M, I>(input: I) -> i32
    where
        DB: Database,
        R: Unpin + 'a,
        M: Unpin + 'a,
        &'a R: Executor<'a, Database = DB>,
        &'a mut M: Executor<'a, Database = DB>,
        I: Into<Wrapper<'a, R, M>>,
    {
        let wrapper = input.into();
        //let mut conn = wrapper.acquire().await.unwrap();

        /*sqlx::query("select 1 + 1")
            .try_map(|row: DB::Row| row.try_get::<i32, _>(0))
            .fetch_one(&mut *conn)
            .await
            .unwrap()*/

        0
    }
}
