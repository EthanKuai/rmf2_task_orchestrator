/*
 * Copyright (C) 2026 ROS-Industrial Consortium Asia Pacific
 * Advanced Remanufacturing and Technology Centre
 * A*STAR Research Entities (Co. Registration No. 199702110H)
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use crossflow::bevy_ecs;
use std::future::Future;
use std::pin::Pin;

#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Connection error: {0}")]
    Connect(String),
    #[error("Publishing error: {0}")]
    Publish(String),
    #[error("Subscribing error: {0}")]
    Subscribe(String),
    #[error("Session not found: {0}")]
    SessionNotFound(String),
}

fn get_type<T: ?Sized>() -> &'static str {
    std::any::type_name::<T>()
}

// -----------------------------------------------------------------

// Send: Arc<Mutex<_>>
// DeserializeOwned + Default: load_base_configuration
/// Handle loading of protocol configuration.
pub trait ProtoSettings: serde::de::DeserializeOwned + Default + Send {
    const TOML_NAME: &'static str;

    fn load_config() -> Result<Self, ProtoError> {
        match crate::config::load_configuration_section::<Self>(Self::TOML_NAME) {
            Ok(settings) => {
                if !settings.is_valid() {
                    return Err(ProtoError::Config(format!(
                        "Invalid [{}] configuration: {}",
                        Self::TOML_NAME,
                        get_type::<Self>()
                    )));
                }
                Ok(settings)
            }
            Err(::config::ConfigError::NotFound(_)) => {
                tracing::warn!(
                    "No [{}] table in configuration, using defaults for {}",
                    Self::TOML_NAME,
                    get_type::<Self>()
                );
                Ok(Self::default())
            }
            Err(e) => Err(ProtoError::Config(format!(
                "Failed to load [{}] into {}: {e}",
                Self::TOML_NAME,
                get_type::<Self>()
            ))),
        }
    }
    fn is_valid(&self) -> bool;
}

/// Constructs [`ProtoSettings`].
///
/// # Example
///
/// ```
/// # #[macro_use] extern crate rmf2_task_orchestrator;
/// # use rmf2_task_orchestrator::client::protocol;
/// protocol::settings! {
///     pub struct MQTTSettings in "mqtt_client" {
///         host: String = "localhost".into(),
///         port: u16 = 1883,
///         client_id: String = "p1".into(),
///     }
///     is_valid |s| {
///         if s.client_id.is_empty() || s.host.is_empty() {
///             return false;
///         }
///         true
///     }
/// }
/// ```
#[doc(hidden)]
#[macro_export]
macro_rules! __proto_settings {
    (
        $(#[$m:meta])*
        $v:vis struct $name:ident in $table:literal {
            $($(#[$fm:meta])* $f:ident : $t:ty = $d:expr),* $(,)?
        }
        is_valid | $s:ident | $body:block
    ) => {
        $crate::__paste::paste! {
            #[doc(hidden)]
            #[allow(non_camel_case_types, unused_imports)]
            use $crate::__serde as [<__rmf2_to_serde_ $name>];

            $(#[$m])*
            #[derive($crate::__serde::Deserialize, Clone, PartialEq, Debug)]
            #[serde(crate = "__rmf2_to_serde_" $name, default, deny_unknown_fields)]
            $v struct $name {
                $($(#[$fm])* pub $f: $t),*
            }
        }

        impl ::core::default::Default for $name {
            fn default() -> Self {
                Self { $($f: $d),* }
            }
        }

        impl $crate::client::protocol::ProtoSettings for $name {
            const TOML_NAME: &'static str = $table;

            fn is_valid(&self) -> bool {
                let $s = self;
                $body
            }
        }
    };
}

#[doc(inline)]
pub use __proto_settings as settings;

// -----------------------------------------------------------------

/// [`publish`][ProtoHandle::publish], [`subscribe`][ProtoHandle::subscribe], [`recv`][ProtoStream::recv] output.
pub type ProtoFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

// bevy_ecs::prelude::Resource: Send + Sync + 'static
// Clone: for Res<bevy_ecs::prelude::Resource>::clone
/// Handles protocol connection, providing pub/sub/connect.
/// Surround original struct with [`handle!`] prior to `impl`.
#[allow(clippy::type_complexity)]
pub trait ProtoHandle: bevy_ecs::prelude::Resource + Clone {
    type Settings: ProtoSettings;
    type NodeConfig;
    type Input;
    type Output;

    fn connect(settings: Self::Settings, runtime: tokio::runtime::Handle) -> Self
    where
        Self: Sized;

    fn publish(
        &self,
        address: &str,
        payload: Self::Input,     // &[u8]
        config: Self::NodeConfig, // &serde_json::Value
    ) -> ProtoFuture<'_, Result<(), ProtoError>>;

    fn subscribe(
        &self,
        address: &str,
        config: Self::NodeConfig, // &serde_json::Value
    ) -> ProtoFuture<'_, Result<Box<dyn ProtoStream<Output = Self::Output>>, ProtoError>>;
}

/// Prerequisites for [`ProtoHandle`].
///
/// # Example
///
/// ```
/// # #[macro_use] extern crate rmf2_task_orchestrator;
/// # use rmf2_task_orchestrator::client::protocol;
/// protocol::handle! {
///     pub struct MQTTHandle {
///         client: Arc<AsyncClient>,
///         subscriptions: Arc<DashMap<String, broadcast::Sender<MqttMessage>>>,
///     }
/// }
/// impl MQTTHandle {
///     fn parse_qos(qos: u8) -> Result<rumqttc::QoS, protocol::ProtoError> {
///         Ok(match qos {
///             0 => rumqttc::QoS::AtMostOnce,
///             _ => return Err(protocol::ProtoError::Config(qos)),
///         })
///     }
/// }
/// impl protocol::ProtoHandle for MQTTHandle {
///     type Settings = MQTTSettings;
///     type NodeConfig = Vec<u8>;
///     type Input = Vec<u8>;
///     type Output = Vec<u8>;
///
///     fn connect(config: Self::Settings) -> Self {
///         ...
///     }
///
///     fn subscribe(&self, topic: &str, qos: u8) -> ProtoFuture<'_, ()> {
///         ...
///     }
///
///     fn publish(&self, topic: &str, qos: u8, payload: Self::Input) -> ProtoFuture<'_, Box<dyn ProtoStream<IoType = Self::Output>>> {
///         ...
///     }
/// }
/// ```
#[doc(hidden)]
#[macro_export]
macro_rules! __proto_handle {
    (
        $(#[$m:meta])*
        $v:vis struct $n:ident { $($f:ident : $t:ty),* $(,)? }
    ) => {
        $(#[$m])*
        #[derive(Clone)]
        $v struct $n { $(pub $f: $t),* }

        // Implement Resource trait manually to prevent versioning issues
        // Simple `#[derive(Resource)]` uses downstream version
        impl $crate::__bevy_ecs::prelude::Resource for $n {}
    };
}

#[doc(inline)]
pub use __proto_handle as handle;

// -----------------------------------------------------------------

/// [`type Output`][ProtoStream::Output]: recommend [`Vec<u8>`] or [`serde_json::Value`]. Any other should be a custom type.
pub trait ProtoStream: Send + 'static {
    type Output;
    fn recv(&mut self) -> ProtoFuture<'_, Option<Self::Output>>;
}
