#![allow(clippy::needless_return)]

use thruster::{
    context::typed_hyper_context::TypedHyperContext, context_state, m, middleware_fn, App,
    HyperRequest, HyperServer, MiddlewareNext, MiddlewareResult, ThrusterServer,
};

mod middleware;
use middleware::rate_limit::{rate_limit_middleware, RateLimiter, RedisStore};

type Context = TypedHyperContext<RequestState>;

#[context_state]
struct RequestState(RateLimiter<RedisStore>);

struct ServerConfig {
    rate_limiter: RateLimiter<RedisStore>,
}

fn init_context(request: HyperRequest, state: &ServerConfig, _path: &str) -> Context {
    let ServerConfig { rate_limiter } = state;
    return Context::new(request, RequestState(rate_limiter.clone()));
}

#[middleware_fn]
async fn root(mut context: Context, _next: MiddlewareNext<Context>) -> MiddlewareResult<Context> {
    context.body("hi");

    return Ok(context);
}

#[tokio::main]
async fn main() {
    let rate_limiter = RateLimiter {
        max: 100,
        per_s: 60,
        store: RedisStore::new("redis://127.0.0.1").await.unwrap(),
    };

    let app = App::<HyperRequest, Context, ServerConfig>::create(
        init_context,
        ServerConfig { rate_limiter },
    )
    .middleware("/", m![rate_limit_middleware])
    .get("/", m![root]);

    let server = HyperServer::new(app);
    server.build("127.0.0.1", 3000).await;
}
