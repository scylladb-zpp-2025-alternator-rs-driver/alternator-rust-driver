//! This is the simplest possible implementation of a custom http_client.
use crate::alternator_client::routing_scope::RoutingScope;
use aws_smithy_async::rt::sleep::default_async_sleep;
use aws_smithy_runtime::client::http::hyper_014::default_connector;
use aws_smithy_runtime_api::client::orchestrator::HttpRequest;

use aws_smithy_runtime_api::client::http::{
    HttpClient, HttpConnector, HttpConnectorFuture, HttpConnectorSettings,
    HttpConnectorSettingsBuilder, SharedHttpConnector,
};

use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;

#[derive(Debug)]
struct AlternatorHttpConnector {
    inner: SharedHttpConnector,
}

impl AlternatorHttpConnector {
    fn new() -> Self {
        let settings = HttpConnectorSettingsBuilder::default().build();
        let connector = default_connector(&settings, default_async_sleep()).unwrap();
        Self { inner: connector }
    }
}

impl HttpConnector for AlternatorHttpConnector {
    fn call(&self, request: HttpRequest) -> HttpConnectorFuture {
        self.inner.call(request)
    }
}

#[derive(Debug)]
pub struct AlternatorHttpClient {
    connector: SharedHttpConnector,
}

impl AlternatorHttpClient {
    pub fn builder() -> AlternatorHttpClientBuilder {
        AlternatorHttpClientBuilder::default()
    }
}

#[derive(Default)]
pub struct AlternatorHttpClientBuilder {
    routing_scope: Option<RoutingScope>,
}

impl AlternatorHttpClientBuilder {
    pub fn with_routing_scope(mut self, routing_scope: RoutingScope) -> Self {
        self.routing_scope = Some(routing_scope);
        self
    }
    pub fn build(self) -> AlternatorHttpClient {
        // For now we just ignore the scope and return the default client.
        AlternatorHttpClient {
            connector: SharedHttpConnector::new(AlternatorHttpConnector::new()),
        }
    }
}

impl HttpClient for AlternatorHttpClient {
    fn http_connector(
        &self,
        _settings: &HttpConnectorSettings,
        _components: &RuntimeComponents,
    ) -> SharedHttpConnector {
        self.connector.clone()
    }
}
