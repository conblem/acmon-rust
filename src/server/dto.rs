use serde::{Serialize, Serializer};
use std::marker::PhantomData;

#[derive(Serialize)]
pub(crate) struct SignedRequest<P, S> {
    protected: String,
    payload: Payload<P>,
    signature: S,
}

struct Payload<P> {
    inner: String,
    phantom: PhantomData<P>,
}

impl<P> Serialize for Payload<P> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.serialize(serializer)
    }
}
