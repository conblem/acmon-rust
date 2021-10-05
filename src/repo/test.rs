use core::convert::Infallible;

trait SayHallo {
    fn hi(self) -> String;
}

struct MutSayer;

impl SayHallo for &'_ mut MutSayer {
    fn hi(self) -> String {
        "This is a MutSayer".to_string()
    }
}

struct RefSayer;

impl SayHallo for &'_ RefSayer {
    fn hi(self) -> String {
        "This is a RefSayer".to_string()
    }
}

enum Wrapper<'a, R, M> {
    Ref { inner: &'a R },
    Mut { inner: &'a mut M },
}

impl<'a, 'b, R, M> Wrapper<'a, R, M>
where
    R: 'b,
    M: 'b,
    Wrapper<'b, R, M>: IntoInner,
{
    fn get_mut(&'b mut self) -> <Wrapper<'b, R, M> as IntoInner>::Target {
        let wrapper: Wrapper<'b, R, M> = match self {
            Wrapper::Ref { inner } => Wrapper::Ref { inner },
            Wrapper::Mut { inner } => Wrapper::Mut { inner },
        };
        wrapper.inner()
    }
}

impl<'a, R> From<&'a R> for Wrapper<'a, R, Infallible> {
    fn from(inner: &'a R) -> Self {
        Wrapper::Ref { inner }
    }
}

impl<'a, M> From<&'a mut M> for Wrapper<'a, Infallible, M> {
    fn from(inner: &'a mut M) -> Self {
        Wrapper::Mut { inner }
    }
}

trait IntoInner {
    type Target: SayHallo;
    fn inner(self) -> Self::Target;
}

impl<'a, M> IntoInner for Wrapper<'a, Infallible, M>
where
    &'a mut M: SayHallo,
{
    type Target = &'a mut M;

    fn inner(self) -> &'a mut M {
        match self {
            Wrapper::Ref { inner } => match *inner {},
            Wrapper::Mut { inner } => inner,
        }
    }
}

impl<'a, R> IntoInner for Wrapper<'a, R, Infallible>
where
    &'a R: SayHallo,
{
    type Target = &'a R;

    fn inner(self) -> &'a R {
        match self {
            Wrapper::Ref { inner } => inner,
            Wrapper::Mut { inner } => match *inner {},
        }
    }
}

fn main() {
    let mut mut_sayer = MutSayer;
    let res = generic_hi(&mut mut_sayer);
    println!("{}", res);

    let ref_sayer = RefSayer;
    let res = generic_hi(&ref_sayer);
    println!("{}", res);
}

fn generic_hi<'a, R, M, I>(say_hallo: I) -> String
where
    R: 'a,
    M: 'a,
    I: Into<Wrapper<'a, R, M>>,
    for<'b> Wrapper<'b, R, M>: IntoInner,
{
    let mut wrapper = say_hallo.into();
    let res = wrapper.get_mut().hi();
    let res_two = wrapper.get_mut().hi();

    format!("{}, {}", res, res_two)
}
