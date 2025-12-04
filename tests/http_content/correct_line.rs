//! Correct Request Line Test
//! This test asserts driver generates only requests with correct line: Method = POST, URI = "/".
//! We use a proxy to intercept messages sent between driver and alternator.
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
use alternator_driver::types::{
    AttributeDefinition, AttributeValue, BillingMode, KeySchemaElement, KeyType,
    ScalarAttributeType,
};

struct Config;
impl HttpTestConfig for Config {
    async fn on_request(
        request: Request<Incoming>,
        sender: Arc<Mutex<SendRequest<Full<Bytes>>>>,
    ) -> Response<Full<Bytes>> {
        let (parts, body) = collect_request(request).await;

        // check HTTP line correctness: POST /
        assert_eq!(
            parts.method.as_str(),
            "POST",
            "Unexpected HTTP request method"
        );
        assert_eq!(
            parts.uri,
            http::Uri::from_static("/"),
            "Unexpected HTTP request URI"
        );

        // forward
        let (parts, body) = collect_received_response(parts, body, sender).await;
        build_response(parts, body)
    }

    async fn cleanup(resources: Vec<String>, alternator_address: &str) {
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
            // try to delete
            let mut result = client.delete_table().table_name(&resource).send().await;

            // wait if resource is not yet ready
            if let Err(ref e) = result
                && e.as_service_error()
                    .is_some_and(|s| s.is_resource_in_use_exception())
            {
                client
                    .wait_until_table_exists()
                    .table_name(&resource)
                    .wait(Duration::from_secs(5))
                    .await
                    .unwrap();

                result = client.delete_table().table_name(&resource).send().await;
            }

            // final check
            if let Err(e) = result
                && !e
                    .as_service_error()
                    .is_some_and(|s| s.is_resource_not_found_exception())
            {
                panic!("Cleanup failed: {e:?}");
            }
        }
    }
}

#[test_context(HttpTestContext<Config>)]
#[tokio::test]
pub async fn test(ctx: &mut HttpTestContext<ContextConfig>) {
    // create client
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
