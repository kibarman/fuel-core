#[cfg(feature = "metrics")]
use crate::graphql_api::prometheus::PrometheusExtension;
use crate::{
    fuel_core_graphql_api::ports::{
        BlockProducerPort,
        ConsensusModulePort,
        DatabasePort,
        TxPoolPort,
    },
    graphql_api::{
        honeycomb::HoneyTrace,
        Config,
    },
    schema::{
        CoreSchema,
        CoreSchemaBuilder,
    },
    service::metrics::metrics,
};
use async_graphql::{
    extensions::Tracing,
    http::{
        playground_source,
        GraphQLPlaygroundConfig,
    },
    Request,
    Response,
};
use axum::{
    extract::{
        DefaultBodyLimit,
        Extension,
    },
    http::{
        header::{
            ACCESS_CONTROL_ALLOW_HEADERS,
            ACCESS_CONTROL_ALLOW_METHODS,
            ACCESS_CONTROL_ALLOW_ORIGIN,
        },
        HeaderValue,
    },
    response::{
        sse::Event,
        Html,
        IntoResponse,
        Sse,
    },
    routing::{
        get,
        post,
    },
    Json,
    Router,
};
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    StateWatcher,
};
use futures::Stream;
use serde_json::json;
use std::{
    future::Future,
    net::{
        SocketAddr,
        TcpListener,
    },
    pin::Pin,
};
use tokio_stream::StreamExt;
use tower_http::{
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

pub type Service = fuel_core_services::ServiceRunner<NotInitializedTask>;

pub type Database = Box<dyn DatabasePort>;
pub type BlockProducer = Box<dyn BlockProducerPort>;
// In the future GraphQL should not be aware of `TxPool`. It should
//  use only `Database` to receive all information about transactions.
pub type TxPool = Box<dyn TxPoolPort>;
pub type ConsensusModule = Box<dyn ConsensusModulePort>;

#[derive(Clone)]
pub struct SharedState {
    pub bound_address: SocketAddr,
}

pub struct NotInitializedTask {
    router: Router,
    listener: TcpListener,
    bound_address: SocketAddr,
}

pub struct Task {
    // Ugly workaround because of https://github.com/hyperium/hyper/issues/2582
    server: Pin<Box<dyn Future<Output = hyper::Result<()>> + Send + 'static>>,
}

#[async_trait::async_trait]
impl RunnableService for NotInitializedTask {
    const NAME: &'static str = "GraphQL";

    type SharedData = SharedState;
    type Task = Task;

    fn shared_data(&self) -> Self::SharedData {
        SharedState {
            bound_address: self.bound_address,
        }
    }

    async fn into_task(self, state: &StateWatcher) -> anyhow::Result<Self::Task> {
        let mut state = state.clone();
        let server = axum::Server::from_tcp(self.listener)
            .unwrap()
            .serve(self.router.into_make_service())
            .with_graceful_shutdown(async move {
                state
                    .while_started()
                    .await
                    .expect("The service is destroyed");
            });

        Ok(Task {
            server: Box::pin(server),
        })
    }
}

#[async_trait::async_trait]
impl RunnableTask for Task {
    async fn run(&mut self, _: &mut StateWatcher) -> anyhow::Result<bool> {
        self.server.as_mut().await?;
        // The `axum::Server` has its internal loop. If `await` is finished, we get an internal
        // error or stop signal.
        Ok(false /* should_continue */)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        // Nothing to shut down because we don't have any temporary state that should be dumped,
        // and we don't spawn any sub-tasks that we need to finish or await.
        // The `axum::Server` was already gracefully shutdown at this point.
        Ok(())
    }
}

pub fn new_service(
    config: Config,
    database: Database,
    schema: CoreSchemaBuilder,
    producer: BlockProducer,
    txpool: TxPool,
    consensus_module: ConsensusModule,
) -> anyhow::Result<Service> {
    let network_addr = config.addr;

    let honeycomb_enabled = config.honeycomb_enabled;

    let builder = schema
        .data(config)
        .data(database)
        .data(txpool)
        .data(producer)
        .data(consensus_module);
    // use honeycomb tracing wrapper if api key is configured
    let builder = if honeycomb_enabled {
        builder.extension(HoneyTrace)
    } else {
        builder.extension(Tracing)
    };

    #[cfg(feature = "metrics")]
    let builder = builder.extension(PrometheusExtension {});

    let schema = builder.finish();

    let router = Router::new()
        .route("/playground", get(graphql_playground))
        .route("/graphql", post(graphql_handler).options(ok))
        .route(
            "/graphql-sub",
            post(graphql_subscription_handler).options(ok),
        )
        .route("/metrics", get(metrics))
        .route("/health", get(health))
        .layer(Extension(schema))
        .layer(TraceLayer::new_for_http())
        .layer(SetResponseHeaderLayer::<_>::overriding(
            ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        ))
        .layer(SetResponseHeaderLayer::<_>::overriding(
            ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("*"),
        ))
        .layer(SetResponseHeaderLayer::<_>::overriding(
            ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("*"),
        ))
        .layer(DefaultBodyLimit::disable());

    let listener = TcpListener::bind(network_addr)?;
    let bound_address = listener.local_addr()?;

    tracing::info!("Binding GraphQL provider to {}", bound_address);

    Ok(Service::new(NotInitializedTask {
        router,
        listener,
        bound_address,
    }))
}

async fn graphql_playground() -> impl IntoResponse {
    Html(playground_source(GraphQLPlaygroundConfig::new("/graphql")))
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "up": true }))
}

async fn graphql_handler(
    schema: Extension<CoreSchema>,
    req: Json<Request>,
) -> Json<Response> {
    schema.execute(req.0).await.into()
}

async fn graphql_subscription_handler(
    schema: Extension<CoreSchema>,
    req: Json<Request>,
) -> Sse<impl Stream<Item = anyhow::Result<Event, serde_json::Error>>> {
    let stream = schema
        .execute_stream(req.0)
        .map(|r| Ok(Event::default().json_data(r).unwrap()));
    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().text("keep-alive-text"))
}

async fn ok() -> anyhow::Result<(), ()> {
    Ok(())
}
