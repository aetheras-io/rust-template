use super::claims::Claims;
use crate::{graphql::AppSchema, opts::Encoder};

use async_graphql::{
    Data, Response as GResponse, ServerError,
    http::{ALL_WEBSOCKET_PROTOCOLS, GraphiQLSource},
};
use async_graphql_axum::{GraphQLProtocol, GraphQLRequest, GraphQLResponse, GraphQLWebSocket};
use axum::{
    Router,
    extract::{State, WebSocketUpgrade},
    http::header::AUTHORIZATION,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use tracing::instrument;

pub fn routes() -> Router<crate::api::state::AppState> {
    base_routes().merge(dev_routes())
}

pub fn base_routes() -> Router<crate::api::state::AppState> {
    Router::new().route("/graphql", post(graphql_handler))
}

/// Only debug builds have graphiql and a noauth path
#[cfg(debug_assertions)]
pub fn dev_routes() -> Router<crate::api::state::AppState> {
    Router::new()
        .route("/graphql", get(graphiql))
        .route("/graphql_noauth", post(graphql_handler_no_auth))
}

#[cfg(not(debug_assertions))]
pub fn dev_routes() -> Router<crate::api::state::AppState> {
    Router::new()
}

#[instrument(
      skip(schema, claims, req),
      fields(
          user_id = tracing::field::Empty,
          gql_operation = tracing::field::Empty,
          gql_operation_type = tracing::field::Empty,
      )
  )]
async fn graphql_handler(
    State(schema): State<AppSchema>,
    claims: Claims,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let span = tracing::Span::current();
    let op_type = req.0.query.split_whitespace().next().unwrap_or("query");
    let op_name = req
        .0
        .operation_name
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("op");
    span.record(
        "gql_operation",
        tracing::field::display([op_type, "::", op_name].concat()),
    );

    let mut req = req.into_inner();
    let claims = claims.into_inner();
    let Ok(subject) = claims.subject_as_uuid() else {
        return GResponse::from_errors(vec![ServerError::new(
            "unable to serialize UUID in claims subject".to_owned(),
            None,
        )])
        .into();
    };
    span.record("user_id", tracing::field::display(subject));

    req = req.data(claims);
    req = req.data(subject);

    let resp = schema.execute(req).await;
    if !resp.errors.is_empty() {
        tracing::error!(
            target: "graphql",
            errors = ?resp.errors,
            count = ?resp.errors.len(),
        );
    }
    resp.into()
}

#[allow(unused)]
async fn graphql_ws_handler(
    State(schema): State<AppSchema>,
    protocol: GraphQLProtocol,
    websocket: WebSocketUpgrade,
) -> Response {
    websocket
        .protocols(ALL_WEBSOCKET_PROTOCOLS)
        .on_upgrade(move |stream| {
            GraphQLWebSocket::new(stream, schema.clone(), protocol)
                .on_connection_init(|v| async move {
                    //#NOTE Verification / auth handled here
                    tracing::info!("{v}");
                    Ok(Data::default())
                })
                .serve()
        })
}

async fn graphiql(State(encoder): State<Encoder>) -> impl IntoResponse {
    use atb::fixtures::ALICE;
    use atb_types::Duration;

    let (claims, _, _) = encoder
        .claims_encoded(ALICE.to_string(), vec![], Duration::days(365), ())
        .unwrap();

    Html(
        GraphiQLSource::build()
            .header(AUTHORIZATION.as_str(), &format!("Bearer {claims}"))
            .endpoint("/graphql")
            .subscription_endpoint("/graphql/ws")
            .finish(),
    )
}

async fn graphql_handler_no_auth(
    State(schema): State<AppSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    tracing::info!("graphql no auth operation: {:?}", req.0.operation_name);
    schema.execute(req.into_inner()).await.into()
}
