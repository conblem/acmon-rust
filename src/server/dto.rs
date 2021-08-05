use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::convert::TryInto;
use std::fmt;
use std::marker::PhantomData;

const fn default_false() -> bool {
    false
}

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

#[derive(Debug)]
pub(crate) struct Uri(hyper::Uri);

impl Into<hyper::Uri> for &Uri {
    fn into(self) -> hyper::Uri {
        self.0.clone()
    }
}

impl Serialize for Uri {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(format!("{}", &self.0).as_str())
    }
}

struct UriVisitor;

impl<'de> Visitor<'de> for UriVisitor {
    type Value = Uri;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("An URI")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match v.parse() {
            Ok(uri) => Ok(Uri(uri)),
            Err(err) => Err(E::custom(err)),
        }
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match v.try_into() {
            Ok(uri) => Ok(Uri(uri)),
            Err(err) => Err(E::custom(err)),
        }
    }
}

impl<'de> Deserialize<'de> for Uri {
    fn deserialize<D>(deserializer: D) -> Result<Uri, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(UriVisitor)
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApiDirectory {
    pub(crate) new_nonce: Uri,
    pub(crate) new_account: Uri,
    pub(crate) new_order: Uri,
    pub(crate) new_authz: Option<Uri>,
    pub(crate) revoke_cert: Uri,
    pub(crate) key_change: Uri,
    pub(crate) meta: Option<ApiMeta>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApiMeta {
    terms_of_service: Option<String>,
    website: Option<String>,
    #[serde(default)]
    caa_identities: Vec<String>,
    #[serde(default = "default_false")]
    external_account_required: bool,
}
