use crate::*;

use aws_sdk_dynamodb::client::customize::CustomizableOperation;

/// Trait to be implemented by Dynamodb's [CustomizableOperation].
///
/// It allows us to override [AlternatorConfig] at per-operation level, like so:
/// ```ignore
/// client
///     .create_table()
///     // ...
///     .customize()
///     .alternator_config_override(
///         AlternatorConfig::builder()
///             .behavior_version_latest()
///             .build()
///     )
///     .send()
///     .await
///     .unwrap();
/// ```
pub trait AlternatorCustomizableOperation<T, E, B> {
    fn alternator_config_override(self, config_override: impl Into<AlternatorBuilder>) -> Self;
}

impl<T, E, B> AlternatorCustomizableOperation<T, E, B> for CustomizableOperation<T, E, B> {
    fn alternator_config_override(self, config_override: impl Into<AlternatorBuilder>) -> Self {
        let config_override: AlternatorBuilder = config_override.into();
        self.config_override(config_override.dynamodb_builder)
    }
}
