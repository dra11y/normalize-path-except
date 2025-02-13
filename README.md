# tower_http::NormalizePath, but with exceptions

NormalizePath from tower, but with exceptions, because it's hard to add them when you have to wrap your whole service in this middleware for axum.

Middleware that normalizes paths, with exceptions.

Forked with minimal changes from tower_http::NormalizePathLayer.

Any trailing slashes from request paths, _except those paths **starting with** one of the entries in `exceptions`_, will be removed. For example, a request with `/foo/`
will be changed to `/foo` before reaching the inner service.

# Example

```rs
use normalize_path_except::NormalizePathLayer;
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use bytes::Bytes;
use std::{iter::once, convert::Infallible};
use tower::{ServiceBuilder, Service, ServiceExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    async fn handle(req: Request<Full<Bytes>>) -> Result<Response<Full<Bytes>>, Infallible> {
        // `req.uri().path()` will not have trailing slashes
        # Ok(Response::new(Full::default()))
    }

    let mut service = ServiceBuilder::new()
        // trim trailing slashes from paths except those **starting with** /swagger-ui
        .layer(NormalizePathLayer::trim_trailing_slash(&["/swagger-ui"]))
        .service_fn(handle);

    // call the service
    let request = Request::builder()
        // `handle` will see `/foo`
        .uri("/foo/")
        .body(Full::default())?;

    service.ready().await?.call(request).await?;
    Ok(())
}
```
