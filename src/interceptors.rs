use crate::*;

use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::interceptors::Intercept;
use aws_smithy_runtime_api::client::interceptors::context::{
    BeforeSerializationInterceptorContextMut, BeforeTransmitInterceptorContextMut,
};
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_types::config_bag::ConfigBag;
use aws_smithy_types::config_bag::{Storable, StoreReplace};

/// Driver's main interceptor
///
/// Is added by [AlternatorClient] to its inner Dynamodb client on construction.
///
/// Uses [strip_headers].
///
/// Also checks [ConfigBag] for config overrides that could have been left by [AlternatorOverrideInterceptor].
#[derive(Debug)]
pub(crate) struct AlternatorInterceptor {
    enforce_header_whitelist: bool,
}
impl AlternatorInterceptor {
    pub fn new(enforce_header_whitelist: bool) -> Self {
        Self {
            enforce_header_whitelist,
        }
    }
}
impl Intercept for AlternatorInterceptor {
    fn name(&self) -> &'static str {
        "AlternatorInterceptor"
    }

    fn modify_before_transmit(
        &self,
        context: &mut BeforeTransmitInterceptorContextMut,
        _: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        // check for overrides
        let enforce_header_whitelist = cfg
            .interceptor_state()
            .load::<EnforceHeaderWhitelistStore>()
            .map(|store| store.enforce_header_whitelist)
            .unwrap_or(self.enforce_header_whitelist);

        // enforce header whitelist
        if enforce_header_whitelist {
            strip_headers(context.request_mut());
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EnforceHeaderWhitelistStore {
    enforce_header_whitelist: bool,
}
impl Storable for EnforceHeaderWhitelistStore {
    type Storer = StoreReplace<Self>;
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
impl AlternatorOverrideInterceptor<EnforceHeaderWhitelistStore> {
    pub(crate) fn for_enforce_header_whitelist(enforce_header_whitelist: bool) -> Self {
        AlternatorOverrideInterceptor {
            store: EnforceHeaderWhitelistStore {
                enforce_header_whitelist,
            },
        }
    }
}
