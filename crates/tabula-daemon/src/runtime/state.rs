use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use http::HeaderValue;
use tokio::sync::Semaphore;

use crate::kernel::engine::KernelEngine;
use crate::runtime::config::ServerConfig;

/// Shared app state used by handlers and middleware.
#[derive(Clone)]
pub struct AppState {
    config: ServerConfig,
    engine: Arc<dyn KernelEngine>,
    limiter: Arc<Semaphore>,
}

impl AppState {
    pub fn new(config: ServerConfig, engine: Arc<dyn KernelEngine>) -> Self {
        let limiter = Arc::new(Semaphore::new(config.max_concurrent_jobs));
        Self {
            config,
            engine,
            limiter,
        }
    }

    pub fn bind_addr(&self) -> SocketAddr {
        self.config.bind_addr
    }

    pub fn auth_token(&self) -> Option<&str> {
        self.config.auth_token.as_deref()
    }

    pub fn max_body_bytes(&self) -> usize {
        self.config.max_body_bytes
    }

    pub fn allow_origins(&self) -> &[HeaderValue] {
        &self.config.allow_origins
    }

    pub fn engine(&self) -> Arc<dyn KernelEngine> {
        self.engine.clone()
    }

    pub fn limiter(&self) -> Arc<Semaphore> {
        self.limiter.clone()
    }

    pub fn queue_timeout(&self) -> Duration {
        self.config.queue_timeout
    }

    pub fn request_timeout(&self) -> Duration {
        self.config.request_timeout
    }
}
