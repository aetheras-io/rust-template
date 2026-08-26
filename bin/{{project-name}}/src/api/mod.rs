pub mod auth;
pub mod claims;
pub mod errors;
pub mod graphql;
pub mod state;

use crate::opts::HttpOpts;

use std::time::Duration;

use axum::{
    Router,
    extract::{self, FromRequestParts},
    http::{HeaderValue, Method, Request, StatusCode, header},
    middleware::{self, Next},
    routing::get,
};
use axum_client_ip::ClientIp;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

pub fn build_app(opts: &HttpOpts, state: state::AppState) -> anyhow::Result<Router> {
    let service_info = serde_json::to_string_pretty(atb_cli_utils::process_info())?;

    let allowed_origins = opts
        .origins
        .iter()
        .map(|origin| origin.parse::<HeaderValue>())
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Router::new()
        .route(
            "/infoz",
            get(move || {
                let service_info = service_info.clone();
                async move { service_info }
            }),
        )
        .route("/healthz", get(|| async { StatusCode::OK }))
        .nest("/auth", auth::routes())
        .merge(graphql::routes())
        .layer(
            CorsLayer::new()
                .allow_origin(allowed_origins)
                .allow_methods([Method::GET, Method::POST])
                .allow_headers([header::AUTHORIZATION, header::ACCEPT, header::CONTENT_TYPE])
                .allow_credentials(true)
                .max_age(Duration::from_secs(3600)),
        )
        .layer(
            tower::ServiceBuilder::new()
                .layer(opts.client_ip_source.clone().into_extension())
                .layer(
                    TraceLayer::new_for_http().make_span_with(|request: &Request<_>| {
                        tracing::info_span!(
                            "http_request",
                            method = %request.method(),
                            uri = %request.uri(),
                            ip = tracing::field::Empty
                        )
                    }),
                )
                .layer(middleware::from_fn(
                    async |request: extract::Request, next: Next| {
                        let (mut parts, body) = request.into_parts();
                        if let Ok(ip) = ClientIp::from_request_parts(&mut parts, &()).await {
                            let span = tracing::Span::current();
                            span.record("ip", ip.0.to_string());
                        } else {
                            tracing::debug!("client IP unavailable");
                        }
                        next.run(extract::Request::from_parts(parts, body)).await
                    },
                )),
        )
        .with_state(state))
}
