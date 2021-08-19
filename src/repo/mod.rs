mod account;
mod etcd;
mod limit;
mod policy;
mod time;

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fmt;
    use std::fmt::{Debug, Display, Formatter};
    use std::io::{Error as IoError, ErrorKind};
    use std::sync::Arc;

    #[derive(Clone)]
    pub(super) struct BoxError(Arc<dyn Error + Send + Sync + 'static>);

    impl Display for BoxError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            Display::fmt(&self.0, f)
        }
    }

    impl Debug for BoxError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            Debug::fmt(&self.0, f)
        }
    }

    impl Error for BoxError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&*self.0)
        }
    }

    impl From<Box<dyn Error + Send + Sync + 'static>> for BoxError {
        fn from(input: Box<dyn Error + Send + Sync + 'static>) -> Self {
            BoxError(input.into())
        }
    }

    #[test]
    fn debug_works() {
        let err: Box<dyn Error + Send + Sync> = IoError::from(ErrorKind::Other).into();
        assert_eq!(format!("{:?}", err), format!("{:?}", BoxError::from(err)));
    }

    #[test]
    fn display_works() {
        let err: Box<dyn Error + Send + Sync> = IoError::from(ErrorKind::Other).into();
        assert_eq!(format!("{:?}", err), format!("{:?}", BoxError::from(err)));
    }

    #[test]
    fn source_works() {
        let err: Box<dyn Error + Send + Sync> = IoError::from(ErrorKind::Other).into();
        let err: Arc<dyn Error + Send + Sync> = err.into();

        let err_two = err.clone();

        let err = BoxError(err);
        let err = err.source().unwrap();

        // check if source points to original destination
        assert_eq!(err as *const dyn Error, Arc::as_ptr(&err_two))
    }
}
