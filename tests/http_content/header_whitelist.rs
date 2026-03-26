//! Header Whitelist Tests
//! In this module we assert that the driver strips headers from requests when those headers are not used by alternator.
//! We use a proxy to intercept messages sent between driver and alternator.
//!
//! There are 3 test cases:
//! 1. Without Credentials:
//!    Disable use of credentials, then check if all requests follow specific header whitelist:
//!    ["host", "x-amz-target", "content-length", "accept-encoding", "content-encoding"]
//!
//! 2. With Credentials:
//!    Enable use of credentials, then check if all requests follow specific header whitelist:
//!    ["host", "x-amz-target", "content-length", "accept-encoding", "content-encoding", "authorization", "x-amz-date"]
//!
//! 3. Whitelist Needed:
//!    Enable use of credentials, disable header stripping,
//!    then check if unnecessary headers are used at all (therefore we in fact, need to strip them)
//!
//! All 3 use the same set of driver calls, and share the same cleanup function.
use crate::http_content::driver_utils::*;
use crate::http_content::http_test::*;
use crate::http_content::proxy::*;

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::client::conn::http1::SendRequest;
use hyper::{Request, Response};

use std::sync::Arc;
use std::time::Duration;
use test_context::test_context;
use tokio::sync::Mutex;
use uuid::Uuid;

use alternator_driver::client::Waiters;
use alternator_driver::config::auth::{Params, ResolveAuthScheme};
use alternator_driver::config::{ConfigBag, RuntimeComponents};
use alternator_driver::types::{
    AttributeDefinition, AttributeValue, BillingMode, KeySchemaElement, KeyType,
    ScalarAttributeType,
};

use aws_smithy_runtime::client::auth::no_auth::{NO_AUTH_SCHEME_ID, NoAuthScheme};
use aws_smithy_runtime_api::client::auth::{AuthSchemeOption, AuthSchemeOptionsFuture};

async fn make_calls(
    client: &alternator_driver::Client,
    ctx: &mut HttpTestContext<impl HttpTestConfig>,
) {
    // perform driver calls, register any tables to cleanup later
    let table_name = format!("table_{}", Uuid::new_v4());
    ctx.register_resource(table_name.clone());

    client
        .create_table()
        .table_name(&table_name)
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name("ExampleKey")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name("ExampleKey")
                .key_type(KeyType::Hash)
                .build()
                .unwrap(),
        )
        .billing_mode(BillingMode::PayPerRequest)
        .send()
        .await
        .unwrap();

    client
        .put_item()
        .table_name(&table_name)
        .item(
            "ExampleKey",
            AttributeValue::S("ExampleItemKey".to_string()),
        )
        .item(
            "ExampleAttribute",
            AttributeValue::S("ExampleItem".to_string()),
        )
        .send()
        .await
        .unwrap();

    client
        .update_item()
        .table_name(&table_name)
        .key(
            "ExampleKey",
            AttributeValue::S("ExampleItemKey".to_string()),
        )
        .update_expression("SET #d = :v")
        .expression_attribute_names("#d", "ExampleAttribute")
        .expression_attribute_values(":v", AttributeValue::S("ExampleItemUpdated".to_string()))
        .send()
        .await
        .unwrap();

    client
        .get_item()
        .table_name(&table_name)
        .key(
            "ExampleKey",
            AttributeValue::S("ExampleItemKey".to_string()),
        )
        .send()
        .await
        .unwrap();

    client
        .delete_table()
        .table_name(&table_name)
        .send()
        .await
        .unwrap();

    client
        .wait_until_table_not_exists()
        .table_name(&table_name)
        .wait(Duration::from_secs(1))
        .await
        .unwrap();
}

async fn cleanup_calls(resources: Vec<String>, alternator_address: &str) {
    let client = alternator_driver::Client::from_conf(
        alternator_driver::Config::builder()
            .endpoint_url(format!("http://{}", alternator_address))
            .credentials_provider(
                alternator_driver::config::Credentials::for_tests_with_session_token(),
            )
            .region(alternator_driver::config::Region::new("eu-central-1"))
            .behavior_version(alternator_driver::config::BehaviorVersion::latest())
            .build(),
    );

    for resource in resources {
        delete_table_cleanup(&client, &resource).await;
    }
}

#[derive(Debug, Default)]
pub struct NoAuthSchemeResolver;
impl ResolveAuthScheme for NoAuthSchemeResolver {
    fn resolve_auth_scheme<'a>(
        &'a self,
        _: &'a Params,
        _: &'a ConfigBag,
        _: &'a RuntimeComponents,
    ) -> AuthSchemeOptionsFuture<'a> {
        AuthSchemeOptionsFuture::ready(Ok(vec![AuthSchemeOption::from(NO_AUTH_SCHEME_ID)]))
    }
}

struct WithoutCredentialsConfig;
impl HttpTestConfig for WithoutCredentialsConfig {
    async fn on_request(
        request: Request<Incoming>,
        sender: Arc<Mutex<SendRequest<Full<Bytes>>>>,
    ) -> Response<Full<Bytes>> {
        let (parts, body) = collect_request(request).await;

        // allow only whitelisted headers
        let whitelist = [
            "host",
            "x-amz-target",
            "content-length",
            "accept-encoding",
            "content-encoding",
        ];

        let rogue = parts
            .headers
            .keys()
            .find(|header| !whitelist.contains(&header.as_str()));

        assert!(
            rogue.is_none(),
            "Header {:?} not in whitelist: {:#?}",
            rogue.unwrap(),
            whitelist
        );

        // forward
        let (parts, body) = collect_received_response(parts, body, sender).await;
        build_response(parts, body)
    }

    async fn cleanup(resources: Vec<String>, alternator_address: &str) {
        cleanup_calls(resources, alternator_address).await;
    }
}

#[ignore]
#[test_context(HttpTestContext<WithoutCredentialsConfig>)]
#[tokio::test]
pub async fn test_without_credentials(ctx: &mut HttpTestContext<WithoutCredentialsConfig>) {
    // construct client with credentials disabled
    let client = alternator_driver::Client::from_conf(
        alternator_driver::Config::builder()
            .endpoint_url(format!("http://{}", ctx.get_proxy_address()))
            .auth_scheme_resolver(NoAuthSchemeResolver)
            .push_auth_scheme(NoAuthScheme::new())
            .region(alternator_driver::config::Region::new("eu-central-1"))
            .behavior_version(alternator_driver::config::BehaviorVersion::latest())
            .build(),
    );

    // perform calls to alternator, use proxy to peek and forward requests
    // proxy ensures all requests to have headers stripped according to the whitelist in WithoutCredentialsConfig
    make_calls(&client, ctx).await;
}

struct WithCredentialsConfig;
impl HttpTestConfig for WithCredentialsConfig {
    async fn on_request(
        request: Request<Incoming>,
        sender: Arc<Mutex<SendRequest<Full<Bytes>>>>,
    ) -> Response<Full<Bytes>> {
        let (parts, body) = collect_request(request).await;

        // allow only whitelisted headers
        let whitelist = [
            "host",
            "x-amz-target",
            "content-length",
            "accept-encoding",
            "content-encoding",
            "authorization",
            "x-amz-date",
        ];

        let rogue = parts
            .headers
            .keys()
            .find(|header| !whitelist.contains(&header.as_str()));

        assert!(
            rogue.is_none(),
            "Header {:?} not in whitelist: {:#?}",
            rogue.unwrap(),
            whitelist
        );

        // forward
        let (parts, body) = collect_received_response(parts, body, sender).await;
        build_response(parts, body)
    }

    async fn cleanup(resources: Vec<String>, alternator_address: &str) {
        cleanup_calls(resources, alternator_address).await;
    }
}

#[ignore]
#[test_context(HttpTestContext<WithCredentialsConfig>)]
#[tokio::test]
pub async fn test_with_credentials(ctx: &mut HttpTestContext<WithCredentialsConfig>) {
    // construct client with credentials enabled
    let client = alternator_driver::Client::from_conf(
        alternator_driver::Config::builder()
            .endpoint_url(format!("http://{}", ctx.get_proxy_address()))
            .credentials_provider(
                alternator_driver::config::Credentials::for_tests_with_session_token(),
            )
            .region(alternator_driver::config::Region::new("eu-central-1"))
            .behavior_version(alternator_driver::config::BehaviorVersion::latest())
            .build(),
    );

    // perform calls to alternator, use proxy to peek and forward requests
    // proxy ensures all requests to have headers stripped according to the whitelist in WithCredentialsConfig
    make_calls(&client, ctx).await;
}

struct WhitelistNeededConfig;
impl HttpTestConfig for WhitelistNeededConfig {
    async fn on_request(
        request: Request<Incoming>,
        sender: Arc<Mutex<SendRequest<Full<Bytes>>>>,
    ) -> Response<Full<Bytes>> {
        let (parts, body) = collect_request(request).await;

        // check whitelist
        let whitelist = [
            "host",
            "x-amz-target",
            "content-length",
            "accept-encoding",
            "content-encoding",
            "authorization",
            "x-amz-date",
        ];

        let rogue = parts
            .headers
            .keys()
            .find(|header| !whitelist.contains(&header.as_str()));

        assert!(
            rogue.is_some(),
            "All headers are in whitelist: {:#?}",
            whitelist
        );

        // forward
        let (parts, body) = collect_received_response(parts, body, sender).await;
        build_response(parts, body)
    }

    async fn cleanup(resources: Vec<String>, alternator_address: &str) {
        cleanup_calls(resources, alternator_address).await;
    }
}

#[test_context(HttpTestContext<WhitelistNeededConfig>)]
#[tokio::test]
pub async fn test_whitelist_needed(ctx: &mut HttpTestContext<WhitelistNeededConfig>) {
    // construct client with header stripping disabled
    let client = alternator_driver::Client::from_conf(
        alternator_driver::Config::builder()
            .endpoint_url(format!("http://{}", ctx.get_proxy_address()))
            .credentials_provider(
                alternator_driver::config::Credentials::for_tests_with_session_token(),
            )
            .region(alternator_driver::config::Region::new("eu-central-1"))
            .behavior_version(alternator_driver::config::BehaviorVersion::latest())
            .build(),
    );

    // perform calls to alternator, use proxy to peek and forward requests
    // proxy ensures that requests use headers not in whitelist by default (without header stripping enabled)
    make_calls(&client, ctx).await;
}
