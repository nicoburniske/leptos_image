// Checks to see if image request is valid / already cached.
// If new request, it will cache the image and return the cached image.
// If cached, it will return the cached image.
#[cfg(feature = "ssr")]
pub mod handlers {
    use crate::add_image_cache;
    use crate::optimizer::{CachedImage, CachedImageOption, CreateImageError};
    use actix_web::web::Bytes;

    use actix_web::*;

    use axum::response::Response as AxumResponse;
    use axum::{
        body::boxed,
        body::Body,
        extract::State,
        http::{self, Request, Response, Uri},
        response::IntoResponse,
    };

    use leptos::LeptosOptions;
    use std::convert::Infallible;
    use tower::ServiceExt;
    use tower_http::services::fs::ServeFileSystemResponseBody;
    use tower_http::services::ServeDir;

    /// Returns the cached image if it exists.
    ///
    /// Example:
    /// ```
    ///  // Create Axum Router.
    ///  let app = Router::new();
    ///  // Add route for image cache.
    ///  app.route("/cache/image", get(image_cache_handler))
    /// ```

    pub async fn actix_handler(
        req: HttpRequest,
        body: Bytes,
        state: web::Data<LeptosOptions>,
    ) -> HttpResponse {
        let method: hyper::Method = req.method().clone();
        let uri: Uri = req.uri().clone();
        let headers: actix_web::http::header::HeaderMap = req.headers().clone();

        let mut axum_request: Request<Bytes> = Request::builder()
            .method(method)
            .uri(uri)
            .body(body)
            // loop throiugh headers and add them to the request.
            .unwrap();

        for (name, value) in headers.iter() {
            if let Ok(name) = hyper::header::HeaderName::from_bytes(name.as_str().as_bytes()) {
                if let Ok(value) =
                    hyper::header::HeaderValue::from_str(value.to_str().unwrap_or(""))
                {
                    axum_request.headers_mut().insert(name, value);
                }
            }
        }

        let axum_request = axum_request.map(|b| b.into());
        let state: std::sync::Arc<LeptosOptions> = state.into_inner();

        let leptos_options: &LeptosOptions = state.as_ref();

        let state = axum::extract::State(leptos_options.clone());
        let axum_response = image_cache_handler(state, axum_request).await;
        let response = convert_response(axum_response).await;
        let response = response.expect("Failed to convert response");

        response
    }

    pub async fn convert_response(response: impl IntoResponse) -> Result<HttpResponse, Infallible> {
        let response = response.into_response();
        let (parts, body) = response.into_parts();
        let body = match hyper::body::to_bytes(body).await {
            Ok(bytes) => bytes.to_vec(),
            Err(_) => return Ok(HttpResponse::InternalServerError().finish()),
        };
        let headers = parts
            .headers
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_string(),
                    value.to_str().unwrap().to_string(),
                )
            })
            .collect::<Vec<_>>();
        // Append headers to the response
        let mut response = HttpResponse::Ok();
        headers.into_iter().for_each(|(name, value)| {
            response.append_header((name, value));
        });
        let response = response.body(body);
        Ok(response)
    }

    pub async fn image_cache_handler(
        State(options): State<LeptosOptions>,
        req: Request<Body>,
    ) -> AxumResponse {
        let root = options.site_root.clone();
        let cache_result = check_cache_image(req.uri().clone(), &root).await;

        match cache_result {
            Ok(Some(uri)) => {
                let mut response = execute_file_handler(uri, &root).await.unwrap();
                let cache_control =
                    format!("public, stale-while-revalidate, max-age={}", 60 * 60 * 24);
                let headers = response.headers_mut();
                headers.append(
                    http::header::CACHE_CONTROL,
                    http::HeaderValue::from_str(&cache_control).unwrap(),
                );
                response.map(boxed).into_response()
            }

            Ok(None) => Response::builder()
                .status(404)
                .body("Invalid Image.".to_string())
                .unwrap()
                .map(boxed)
                .into_response(),

            Err(e) => {
                log::error!("Failed to create image: {:?}", e);
                Response::builder()
                    .status(500)
                    .body("Error creating image".to_string())
                    .unwrap()
                    .map(boxed)
                    .into_response()
            }
        }
    }

    async fn execute_file_handler(
        uri: Uri,
        root: &str,
    ) -> Result<Response<ServeFileSystemResponseBody>, Infallible> {
        let req = Request::builder()
            .uri(uri.clone())
            .body(Body::empty())
            .unwrap();
        ServeDir::new(root).oneshot(req).await
    }

    async fn check_cache_image(uri: Uri, root: &str) -> Result<Option<Uri>, CreateImageError> {
        let url = uri.to_string();
        let maybe_cache_image = CachedImage::from_url_encoded(&url).ok();

        let maybe_created = {
            if let Some(ref img) = maybe_cache_image {
                Some(img.create_image(root).await)
            } else {
                None
            }
        };

        match maybe_created {
            Some(Ok((file_path, created))) => {
                if created && maybe_cache_image.is_some() {
                    add_file_to_cache(root, maybe_cache_image.unwrap()).await;
                }
                let new_uri = ("/".to_string() + &file_path).parse::<Uri>().unwrap();
                Ok(Some(new_uri))
            }
            Some(Err(err)) => Err(err),
            None => {
                eprintln!("There is no Image for the given url: {}", url);
                Ok(None)
            }
        }
    }

    // When the image is created, it will be added to the cache.
    // Mostly helpful for dev server startup.
    async fn add_file_to_cache(root: &str, image: CachedImage) {
        if let CachedImageOption::Blur(_) = image.option {
            let path = image.get_file_path_from_root(root);

            let created = tokio::fs::read_to_string(path).await.ok();
            if let Some(created) = created {
                add_image_cache([(image, created)]);
            }
            return;
        }
    }
}
