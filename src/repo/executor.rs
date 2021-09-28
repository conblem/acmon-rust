use either::Either;
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use sqlx::database::HasStatement;
use sqlx::{Database, Describe, Error, Execute, Executor};
use std::convert::Infallible;
use std::fmt::Debug;
use std::marker::{PhantomData, PhantomPinned};

#[derive(Debug)]
struct Never<DB> {
    inner: Infallible,
    phantom: PhantomData<DB>,
}

impl <DB> Unpin for Never<DB> {}

impl<'p, DB: Database + Sync> Executor<'p> for &'_ Never<DB> {
    type Database = DB;

    fn fetch_many<'e, 'q: 'e, E: 'q>(
        self,
        _query: E,
    ) -> BoxStream<'e, Result<Either<DB::QueryResult, DB::Row>, Error>>
    where
        E: Execute<'q, Self::Database>,
    {
        match self.inner {}
    }

    fn fetch_optional<'e, 'q: 'e, E: 'q>(
        self,
        _query: E,
    ) -> BoxFuture<'e, Result<Option<DB::Row>, Error>>
    where
        E: Execute<'q, Self::Database>,
    {
        match self.inner {}
    }

    fn prepare_with<'e, 'q: 'e>(
        self,
        _sql: &'q str,
        _parameters: &'e [<Self::Database as Database>::TypeInfo],
    ) -> BoxFuture<'e, Result<<Self::Database as HasStatement<'q>>::Statement, Error>> {
        match self.inner {}
    }

    #[doc(hidden)]
    fn describe<'e, 'q: 'e>(
        self,
        _sql: &'q str,
    ) -> BoxFuture<'e, Result<Describe<Self::Database>, Error>> {
        match self.inner {}
    }
}

impl<'p, DB: Database + Sync> Executor<'p> for &'_ mut Never<DB> {
    type Database = DB;

    fn fetch_many<'e, 'q: 'e, E: 'q>(
        self,
        _query: E,
    ) -> BoxStream<'e, Result<Either<DB::QueryResult, DB::Row>, Error>>
    where
        E: Execute<'q, Self::Database>,
    {
        match self.inner {}
    }

    fn fetch_optional<'e, 'q: 'e, E: 'q>(
        self,
        _query: E,
    ) -> BoxFuture<'e, Result<Option<DB::Row>, Error>>
    where
        E: Execute<'q, Self::Database>,
    {
        match self.inner {}
    }

    fn prepare_with<'e, 'q: 'e>(
        self,
        _sql: &'q str,
        _parameters: &'e [<Self::Database as Database>::TypeInfo],
    ) -> BoxFuture<'e, Result<<Self::Database as HasStatement<'q>>::Statement, Error>> {
        match self.inner {}
    }

    #[doc(hidden)]
    fn describe<'e, 'q: 'e>(
        self,
        _sql: &'q str,
    ) -> BoxFuture<'e, Result<Describe<Self::Database>, Error>> {
        match self.inner {}
    }
}

#[derive(Debug)]
enum Wrapper<'a, R: Unpin, M: Unpin> {
    Ref {
        inner: &'a R,
        phantom: PhantomPinned,
    },
    Mut {
        inner: &'a mut M,
        phantom: PhantomPinned,
    },
}

impl<'p, DB: Database + Sync, R: Debug + Unpin, M: Debug + Unpin> Executor<'p>
    for &'p mut Wrapper<'p, R, M>
where
    for<'c> &'c R: Executor<'c, Database = DB>,
    for<'c> &'c mut M: Executor<'c, Database = DB>,
{
    type Database = DB;

    fn fetch_many<'e, 'q: 'e, E: 'q>(
        self,
        query: E,
    ) -> BoxStream<'e, Result<Either<DB::QueryResult, DB::Row>, Error>>
    where
        'p: 'e,
        E: Execute<'q, Self::Database>,
    {
        match self {
            Wrapper::Ref { inner, phantom: _ } => inner.fetch_many(query),
            Wrapper::Mut { inner, phantom } => inner.fetch_many(query),
        }
    }

    fn fetch_optional<'e, 'q: 'e, E: 'q>(
        self,
        query: E,
    ) -> BoxFuture<'e, Result<Option<DB::Row>, Error>>
    where
        'p: 'e,
        E: Execute<'q, Self::Database>,
    {
        match self {
            Wrapper::Ref { inner, phantom: _ } => inner.fetch_optional(query),
            Wrapper::Mut { inner, phantom: _ } => inner.fetch_optional(query),
        }
    }

    fn prepare_with<'e, 'q: 'e>(
        self,
        sql: &'q str,
        parameters: &'e [<Self::Database as Database>::TypeInfo],
    ) -> BoxFuture<'e, Result<<Self::Database as HasStatement<'q>>::Statement, Error>>
    where
        'p: 'e,
    {
        match self {
            Wrapper::Ref { inner, phantom: _ } => inner.prepare_with(sql, parameters),
            Wrapper::Mut { inner, phantom: _ } => inner.prepare_with(sql, parameters),
        }
    }

    #[doc(hidden)]
    fn describe<'e, 'q: 'e>(
        self,
        sql: &'q str,
    ) -> BoxFuture<'e, Result<Describe<Self::Database>, Error>>
    where
        'p: 'e,
    {
        match self {
            Wrapper::Ref { inner, phantom: _ } => inner.describe(sql),
            Wrapper::Mut { inner, phantom: _ } => inner.describe(sql),
        }
    }
}

impl<'a, DB, R: Unpin> From<&'a R> for Wrapper<'a, R, Never<DB>> {
    fn from(inner: &'a R) -> Self {
        Self::Ref { inner, phantom: PhantomPinned }
    }
}

impl<'a, DB, M: Unpin> From<&'a mut M> for Wrapper<'a, Never<DB>, M> {
    fn from(inner: &'a mut M) -> Self {
        Self::Mut { inner, phantom: PhantomPinned }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::{PgPool, Postgres};

    use super::*;

    #[tokio::test]
    async fn test() {
        let pool = PgPool::connect("hallo").await.unwrap();
        let wrapper = Wrapper::<'_, _, Never<Postgres>>::from(&pool);
        //convert(&pool);
    }

    fn convert<'a, DB: Database, R: Unpin + 'a, M: Unpin + 'a>(input: impl Into<Wrapper<'a, R, M>>)
    where
        for<'c> &'c R: Executor<'c, Database = DB>,
        for<'c> &'c mut M: Executor<'c, Database = DB>,
    {
        input.into();
    }
}
