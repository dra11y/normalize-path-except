//! Middleware that normalizes paths, with exceptions.
//!
//! Forked with minimal changes from tower_http::NormalizePathLayer.
//!
//! Any trailing slashes from request paths will be removed. For example, a request with `/foo/`
//! will be changed to `/foo` before reaching the inner service.
//!
//! # Example
//!
//! ```
//! use normalize_path_except::NormalizePathLayer;
//! use http::{Request, Response, StatusCode};
//! use http_body_util::Full;
//! use bytes::Bytes;
//! use std::{iter::once, convert::Infallible};
//! use tower::{ServiceBuilder, Service, ServiceExt};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! async fn handle(req: Request<Full<Bytes>>) -> Result<Response<Full<Bytes>>, Infallible> {
//!     // `req.uri().path()` will not have trailing slashes
//!     # Ok(Response::new(Full::default()))
//! }
//!
//! let mut service = ServiceBuilder::new()
//!     // trim trailing slashes from paths except `exceptions`
//!     .layer(NormalizePathLayer::trim_trailing_slash(&["/swagger-ui"]))
//!     .service_fn(handle);
//!
//! // call the service
//! let request = Request::builder()
//!     // `handle` will see `/foo`
//!     .uri("/foo/")
//!     .body(Full::default())?;
//!
//! service.ready().await?.call(request).await?;
//! #
//! # Ok(())
//! # }
//! ```

use http::{Request, Response, Uri};
use std::{
    borrow::Cow,
    task::{Context, Poll},
};
use tower_layer::Layer;
use tower_service::Service;

#[allow(unused_macros)]
macro_rules! define_inner_service_accessors {
    () => {
        /// Gets a reference to the underlying service.
        pub fn get_ref(&self) -> &S {
            &self.inner
        }

        /// Gets a mutable reference to the underlying service.
        pub fn get_mut(&mut self) -> &mut S {
            &mut self.inner
        }

        /// Consumes `self`, returning the underlying service.
        pub fn into_inner(self) -> S {
            self.inner
        }
    };
}

/// Layer that applies [`NormalizePath`] which normalizes paths.
///
/// See the [module docs](self) for more details.
#[derive(Debug, Clone)]
pub struct NormalizePathLayer {
    exceptions: Vec<String>,
}

impl NormalizePathLayer {
    /// Create a new [`NormalizePathLayer`].
    ///
    /// Any trailing slashes from request paths will be removed. For example, a request with `/foo/`
    /// will be changed to `/foo` before reaching the inner service.
    pub fn trim_trailing_slash<S: AsRef<str>>(exceptions: &[S]) -> Self {
        let exceptions = exceptions.iter().map(|x| x.as_ref().to_string()).collect();
        NormalizePathLayer { exceptions }
    }
}

impl<S> Layer<S> for NormalizePathLayer {
    type Service = NormalizePath<S>;

    fn layer(&self, inner: S) -> Self::Service {
        NormalizePath::trim_trailing_slash(inner, &self.exceptions)
    }
}

/// Middleware that normalizes paths.
///
/// See the [module docs](self) for more details.
#[derive(Debug, Clone)]
pub struct NormalizePath<S> {
    exceptions: Vec<String>,
    inner: S,
}

impl<S> NormalizePath<S> {
    /// Create a new [`NormalizePath`].
    ///
    /// Any trailing slashes from request paths will be removed. For example, a request with `/foo/`
    /// will be changed to `/foo` before reaching the inner service.
    pub fn trim_trailing_slash<P: AsRef<str>>(inner: S, exceptions: &[P]) -> Self {
        let exceptions = exceptions.iter().map(|x| x.as_ref().to_string()).collect();
        Self { exceptions, inner }
    }

    define_inner_service_accessors!();
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for NormalizePath<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<ReqBody>) -> Self::Future {
        let path = req.uri().path();
        if !self.exceptions.iter().any(|x| path.starts_with(x)) {
            normalize_trailing_slash(req.uri_mut());
        }
        self.inner.call(req)
    }
}

fn normalize_trailing_slash(uri: &mut Uri) {
    if !uri.path().ends_with('/') && !uri.path().starts_with("//") {
        return;
    }

    let new_path = format!("/{}", uri.path().trim_matches('/'));

    let mut parts = uri.clone().into_parts();

    let new_path_and_query = if let Some(path_and_query) = &parts.path_and_query {
        let new_path_and_query = if let Some(query) = path_and_query.query() {
            Cow::Owned(format!("{}?{}", new_path, query))
        } else {
            new_path.into()
        }
        .parse()
        .unwrap();

        Some(new_path_and_query)
    } else {
        None
    };

    parts.path_and_query = new_path_and_query;
    if let Ok(new_uri) = Uri::from_parts(parts) {
        *uri = new_uri;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;
    use tower::{ServiceBuilder, ServiceExt};

    #[tokio::test]
    async fn works() {
        async fn handle(request: Request<()>) -> Result<Response<String>, Infallible> {
            Ok(Response::new(request.uri().to_string()))
        }

        let mut svc = ServiceBuilder::new()
            .layer(NormalizePathLayer::trim_trailing_slash(&["/bar"]))
            .service_fn(handle);

        let body = svc
            .ready()
            .await
            .unwrap()
            .call(Request::builder().uri("/foo/").body(()).unwrap())
            .await
            .unwrap()
            .into_body();

        assert_eq!(body, "/foo");

        let body = svc
            .ready()
            .await
            .unwrap()
            .call(Request::builder().uri("/foo/bar/").body(()).unwrap())
            .await
            .unwrap()
            .into_body();

        assert_eq!(body, "/foo/bar");

        let body = svc
            .ready()
            .await
            .unwrap()
            .call(Request::builder().uri("/bar/").body(()).unwrap())
            .await
            .unwrap()
            .into_body();

        assert_eq!(body, "/bar/");

        let body = svc
            .ready()
            .await
            .unwrap()
            .call(Request::builder().uri("/bar/baz/").body(()).unwrap())
            .await
            .unwrap()
            .into_body();

        assert_eq!(body, "/bar/baz/");
    }

    #[test]
    fn is_noop_if_no_trailing_slash() {
        let mut uri = "/foo".parse::<Uri>().unwrap();
        normalize_trailing_slash(&mut uri);
        assert_eq!(uri, "/foo");
    }

    #[test]
    fn maintains_query() {
        let mut uri = "/foo/?a=a".parse::<Uri>().unwrap();
        normalize_trailing_slash(&mut uri);
        assert_eq!(uri, "/foo?a=a");
    }

    #[test]
    fn removes_multiple_trailing_slashes() {
        let mut uri = "/foo////".parse::<Uri>().unwrap();
        normalize_trailing_slash(&mut uri);
        assert_eq!(uri, "/foo");
    }

    #[test]
    fn removes_multiple_trailing_slashes_even_with_query() {
        let mut uri = "/foo////?a=a".parse::<Uri>().unwrap();
        normalize_trailing_slash(&mut uri);
        assert_eq!(uri, "/foo?a=a");
    }

    #[test]
    fn is_noop_on_index() {
        let mut uri = "/".parse::<Uri>().unwrap();
        normalize_trailing_slash(&mut uri);
        assert_eq!(uri, "/");
    }

    #[test]
    fn removes_multiple_trailing_slashes_on_index() {
        let mut uri = "////".parse::<Uri>().unwrap();
        normalize_trailing_slash(&mut uri);
        assert_eq!(uri, "/");
    }

    #[test]
    fn removes_multiple_trailing_slashes_on_index_even_with_query() {
        let mut uri = "////?a=a".parse::<Uri>().unwrap();
        normalize_trailing_slash(&mut uri);
        assert_eq!(uri, "/?a=a");
    }

    #[test]
    fn removes_multiple_preceding_slashes_even_with_query() {
        let mut uri = "///foo//?a=a".parse::<Uri>().unwrap();
        normalize_trailing_slash(&mut uri);
        assert_eq!(uri, "/foo?a=a");
    }

    #[test]
    fn removes_multiple_preceding_slashes() {
        let mut uri = "///foo".parse::<Uri>().unwrap();
        normalize_trailing_slash(&mut uri);
        assert_eq!(uri, "/foo");
    }
}
