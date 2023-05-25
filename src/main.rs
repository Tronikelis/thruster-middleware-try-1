#![allow(clippy::needless_return)]

use std::sync::Arc;
use thruster::{
    context::typed_hyper_context::TypedHyperContext, context_state, m, middleware_fn, App,
    HyperRequest, HyperServer, MiddlewareNext, MiddlewareResult, ThrusterServer,
};
use thruster_jab::JabDI;
use tokio::sync::Mutex;

mod middleware;
use middleware::rate_limit::{rate_limit_middleware, RateLimiter, RedisStore};

type Context = TypedHyperContext<RequestState>;

context_state!(RequestState => [Arc<JabDI>, RateLimiter<RedisStore>]);

struct ServerState {
    jab: Arc<JabDI>,
    rate_limiter_store: Arc<Mutex<RedisStore>>,
}

fn init_context(request: HyperRequest, state: &ServerState, _path: &str) -> Context {
    let rate_limiter = RateLimiter {
        max: 10,
        per_ms: 10_000,
        store: state.rate_limiter_store.clone(),
    };

    return Context::new(request, (Arc::clone(&state.jab), rate_limiter));
}

#[middleware_fn]
async fn root(mut context: Context, _next: MiddlewareNext<Context>) -> MiddlewareResult<Context> {
    context.body("hi");

    return Ok(context);
}

#[tokio::main]
async fn main() {
    let rate_limiter_store = Arc::new(Mutex::new(
        RedisStore::new("redis://127.0.0.1".to_string())
            .await
            .unwrap(),
    ));

    let app = App::<HyperRequest, Context, ServerState>::create(
        init_context,
        ServerState {
            jab: Arc::new(JabDI::default()),
            rate_limiter_store,
        },
    )
    .middleware("/", m![rate_limit_middleware])
    .get("/", m![root]);

    let server = HyperServer::new(app);
    server.build("127.0.0.1", 3000).await;
}
