use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::interceptors::Intercept;
use aws_smithy_runtime_api::client::interceptors::context::BeforeSerializationInterceptorContextMut;
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_types::config_bag::ConfigBag;
use aws_smithy_types::config_bag::{Storable, StoreReplace};

/// Driver's main interceptor
///
/// Is added by [AlternatorClient] to its inner Dynamodb client on construction.
///
/// Also checks [ConfigBag] for config overrides that could have been left by [AlternatorOverrideInterceptor].
#[derive(Debug)]
pub(crate) struct AlternatorInterceptor {}
impl AlternatorInterceptor {
    pub fn new() -> Self {
        Self {}
    }
}
impl Intercept for AlternatorInterceptor {
    fn name(&self) -> &'static str {
        "AlternatorInterceptor"
    }
}

/// An interceptor used to override [AlternatorClient]'s config.
///
/// Adds specified config overrides to [ConfigBag], so that [AlternatorInterceptor] can later look for it.
///
/// Is used by [AlternatorCustomizableOperation] to allow per-operation customization.
#[derive(Debug)]
pub(crate) struct AlternatorOverrideInterceptor<T: Storable<Storer = StoreReplace<T>> + Clone> {
    store: T,
}
impl<T: Storable<Storer = StoreReplace<T>> + Clone> Intercept for AlternatorOverrideInterceptor<T> {
    fn name(&self) -> &'static str {
        "AlternatorOverrideInterceptor"
    }

    fn modify_before_serialization(
        &self,
        _: &mut BeforeSerializationInterceptorContextMut,
        _: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        // update config bag, so that AlternatorInterceptor will later include the override
        cfg.interceptor_state().store_put(self.store.clone());

        Ok(())
    }
}
