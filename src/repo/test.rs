use core::convert::Infallible;
use sqlx::{Executor, Postgres, Database};

enum NeverRef {}

enum NeverMut {}

enum Wrapper<'a, R, M, N> {
    Ref { inner: &'a R },
    Mut { inner: &'a mut M },
    _Phantom { inner: N }
}

impl<'a, R> From<&'a R> for Wrapper<'a, R, Infallible, NeverRef> {
    fn from(inner: &'a R) -> Self {
        Wrapper::Ref { inner }
    }
}

impl<'a, M> From<&'a mut M> for Wrapper<'a, Infallible, M, NeverMut> {
    fn from(inner: &'a mut M) -> Self {
        Wrapper::Mut { inner }
    }
}

trait IntoInner {
    type Inner;
    fn inner(self) -> Self::Inner;
}

impl<'a, R: Unpin> IntoInner for Wrapper<'a, R, Infallible, NeverRef>
{
    type Inner = &'a R;

    fn inner(self) -> &'a R {
        match self {
            Wrapper::Ref { inner } => inner,
            Wrapper::Mut { inner } => match *inner {},
            Wrapper::_Phantom { inner} => match inner {}
        }
    }
}

impl<'a, M: Unpin> IntoInner for Wrapper<'a, Infallible, M, NeverMut>
{
    type Inner = &'a mut M;

    fn inner(self) -> &'a mut M {
        match self {
            Wrapper::Ref { inner } => match *inner {},
            Wrapper::Mut { inner } => inner,
            Wrapper::_Phantom { inner} => match inner {}
        }
    }
}

trait Bound<'a>: IntoInner<Inner = Self::Target> {
    type Target: Executor<'a, Database = Postgres>;
}

impl <'a, R> Bound<'a> for Wrapper<'a, R, Infallible, NeverRef> where &'a R: Executor<'a, Database = Postgres> {
    type Target = &'a R;
}

impl <'a, M> Bound<'a> for Wrapper<'a, Infallible, M, NeverMut> where &'a mut M: Executor<'a, Database = Postgres> {
    type Target = &'a mut M;
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[tokio::test]
    async fn test() {
        let pool = PgPool::connect("").await.unwrap();
        let wrapper: Wrapper<'_, _, _, _> = (&pool).into();
        execute(wrapper).await;
    }

    async fn execute<'a, R, M, N>(wrapper: Wrapper<'a, R, M, N>)
    where
        Wrapper<'a, R, M, N>: Bound<'a>,
        //for<'b> O: Executor<'b, Database = Postgres>,
    {
    }

    /*fn execute<'a, R, M, I>(executor: I)
    where
        R: 'a,
        M: 'a,
        I: Into<Wrapper<'a, R, M>>,
        for<'b> Wrapper<'b, R, M>: IntoInner<'b>,
    {
        let mut wrapper = executor.into();
    }*/
}
