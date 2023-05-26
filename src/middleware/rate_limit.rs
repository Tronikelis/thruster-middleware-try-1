use async_trait::async_trait;
use redis::{aio::ConnectionManager, AsyncCommands, RedisError, RedisResult};
use std::fmt::Debug;
use thruster::{
    context::typed_hyper_context::TypedHyperContext, errors::ThrusterError, middleware_fn, Context,
    ContextState, MiddlewareNext, MiddlewareResult,
};

#[derive(Clone)]
pub struct RateLimiter<T: Store + Clone + Sync> {
    pub max: usize,
    pub per_s: usize,
    pub store: T,
}

#[async_trait]
pub trait Store {
    type Error: Debug;
    async fn get(&mut self, key: &str) -> Result<Option<usize>, Self::Error>;
    async fn set(&mut self, key: &str, value: usize, expiry_ms: usize) -> Result<(), Self::Error>;
}

#[derive(Clone)]
pub struct RedisStore {
    connection_manager: ConnectionManager,
}

impl RedisStore {
    pub async fn new(url: &str) -> RedisResult<Self> {
        let client = redis::Client::open(url)?;
        let connection_manager = ConnectionManager::new(client).await?;

        return Ok(Self { connection_manager });
    }
}

#[async_trait]
impl Store for RedisStore {
    type Error = RedisError;

    async fn get(&mut self, key: &str) -> Result<Option<usize>, Self::Error> {
        let current: Option<usize> = self.connection_manager.get(key).await?;
        return Ok(current);
    }

    async fn set(&mut self, key: &str, value: usize, expiry_s: usize) -> Result<(), Self::Error> {
        let _: () = self.connection_manager.set_ex(key, value, expiry_s).await?;
        return Ok(());
    }
}

#[middleware_fn]
pub async fn rate_limit_middleware<
    T: Send + ContextState<RateLimiter<G>>,
    G: 'static + Store + Send + Sync + Clone,
>(
    mut context: TypedHyperContext<T>,
    next: MiddlewareNext<TypedHyperContext<T>>,
) -> MiddlewareResult<TypedHyperContext<T>> {
    let rate_limiter: &mut RateLimiter<G> = context.extra.get_mut();
    let RateLimiter { store, max, per_s } = rate_limiter;

    let key = "rate_limit:".to_string()
        + &context
            .hyper_request
            .as_ref()
            .unwrap()
            .ip
            .unwrap()
            .to_string();

    let current_count: Option<usize> = store.get(&key).await.unwrap();

    let current_count = current_count.unwrap_or(0);
    let new_count = current_count + 1;

    if new_count > *max {
        context.status(429);
        return Err(ThrusterError {
            cause: None,
            context,
            message: "Rate limit exceeded".to_string(),
        });
    }

    let _: () = store.set(&key, new_count, *per_s).await.unwrap();

    return next(context).await;
}
