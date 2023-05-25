use crate::RequestStateGetField;
use async_trait::async_trait;
use redis::{aio::Connection, AsyncCommands, RedisError, RedisResult};
use std::{fmt::Debug, sync::Arc};
use thruster::{
    context::typed_hyper_context::TypedHyperContext, errors::ThrusterError, middleware_fn, Context,
    MiddlewareNext, MiddlewareResult,
};
use tokio::sync::Mutex;

pub struct RateLimiter<T: Store> {
    pub max: usize,
    pub per_ms: usize,
    pub store: Arc<Mutex<T>>,
}

#[async_trait]
pub trait Store {
    type Error: Debug;
    async fn get(&mut self, key: &str) -> Result<Option<usize>, Self::Error>;
    async fn set(&mut self, key: &str, value: usize, expiry_ms: usize) -> Result<(), Self::Error>;
}

pub struct RedisStore {
    url: String,
    connection: Connection,
}

impl RedisStore {
    pub async fn new(url: String) -> RedisResult<Self> {
        let client = redis::Client::open(url.as_str())?;
        let connection = client.get_async_connection().await?;
        return Ok(Self { connection, url });
    }
}

#[async_trait]
impl Store for RedisStore {
    type Error = RedisError;

    async fn get(&mut self, key: &str) -> Result<Option<usize>, Self::Error> {
        let current: Option<usize> = self.connection.get(key).await?;
        return Ok(current);
    }

    async fn set(&mut self, key: &str, value: usize, expiry_ms: usize) -> Result<(), Self::Error> {
        let _: () = self.connection.set_ex(key, value, expiry_ms).await?;
        return Ok(());
    }
}

// RequestStateGetField<RateLimiter<G>> needed
#[middleware_fn]
pub async fn rate_limit_middleware<
    T: Send + RequestStateGetField<RateLimiter<G>>,
    G: 'static + Store + Send + Sync,
>(
    mut context: TypedHyperContext<T>,
    next: MiddlewareNext<TypedHyperContext<T>>,
) -> MiddlewareResult<TypedHyperContext<T>> {
    let rate_limiter: &RateLimiter<G> = context.extra.get();

    let RateLimiter { store, max, per_ms } = rate_limiter;

    let store = Arc::clone(&store);
    let mut store = store.lock().await;

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

    let _: () = store.set(&key, new_count, *per_ms).await.unwrap();

    return next(context).await;
}
