use std::time::Duration;

use alternator_driver::*;
use aws_sdk_dynamodb::client::Waiters;
use config::*;
use types::*;

#[tokio::test]
async fn example_integration_test() {
    // set up
    let config = Config::builder()
        .region(Region::new("us-west-2"))
        .endpoint_url("http://localhost:8000")
        .credentials_provider(Credentials::new("d", "u", None, None, "mmy"))
        .behavior_version(BehaviorVersion::latest())
        .build();
    let client = Client::from_conf(config);

    // create table
    client
        .create_table()
        .table_name("ExampleTable")
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name("ExampleAttribute")
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name("ExampleAttribute")
                .key_type(KeyType::Hash)
                .build()
                .unwrap(),
        )
        .billing_mode(BillingMode::PayPerRequest)
        .send()
        .await
        .unwrap();

    client
        .wait_until_table_exists()
        .table_name("ExampleTable")
        .wait(Duration::new(1, 0))
        .await
        .unwrap();

    let tables = client.list_tables().send().await.unwrap();
    assert!(
        tables
            .table_names
            .unwrap()
            .contains(&"ExampleTable".to_string())
    );

    // delete table
    client
        .delete_table()
        .table_name("ExampleTable")
        .send()
        .await
        .unwrap();

    client
        .wait_until_table_not_exists()
        .table_name("ExampleTable")
        .wait(Duration::new(1, 0))
        .await
        .unwrap();

    let tables = client.list_tables().send().await.unwrap();
    let table_names = tables.table_names.unwrap_or_default();
    assert!(!table_names.iter().any(|name| name == "ExampleTable"));
}
