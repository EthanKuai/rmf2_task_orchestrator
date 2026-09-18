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
/// # use rmf2_task_orchestrator::client::protocol::*;
/// settings! {
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

/// [`recv`][ProtoStream::recv] output.
pub type PinBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// [`PinBoxFuture`] wrapper. [`publish`][ProtoHandle::publish] & [`subscribe`][ProtoHandle::subscribe] output.
pub type ProtoFuture<'a, T> = PinBoxFuture<'a, Result<T, ProtoError>>;

// -----------------------------------------------------------------

// bevy_ecs::prelude::Resource: Send + Sync + 'static
// Clone: for Res<bevy_ecs::prelude::Resource>::clone
/// Handles protocol connection, providing pub/sub/connect.
/// Surround original struct with [`handle!`] prior to `impl`.
pub trait ProtoHandle: bevy_ecs::prelude::Resource + Clone {
    type Settings: ProtoSettings;
    type NodeConfig;
    type In;
    type Out;

    fn connect(settings: Self::Settings, runtime: tokio::runtime::Handle) -> Result<Self, ProtoError>
    where
        Self: Sized;

    fn publish(
        &self,
        address: &str,
        payload: Self::In,        // &[u8]
        config: Self::NodeConfig, // &serde_json::Value
    ) -> ProtoFuture<'_, ()>;

    fn subscribe(
        &self,
        address: &str,
        config: Self::NodeConfig, // &serde_json::Value
    ) -> ProtoFuture<'_, Box<dyn ProtoStream<Out = Self::Out>>>;
}

/// Prerequisites for [`ProtoHandle`].
///
/// # Example
///
/// ```
/// # #[macro_use] extern crate rmf2_task_orchestrator;
/// # use rmf2_task_orchestrator::client::protocol::*;
/// # use dashmap::DashMap;
/// # use rumqttc::{AsyncClient,MqttOptions};
/// # use std::sync::Arc;
/// # use tokio::runtime::Handle;
/// # use tokio::sync::broadcast;
/// # use tokio::sync::broadcast::error::RecvError;
/// pub type MqttOut = Vec<u8>;
///
/// pub struct MqttIn {
///     payload: MqttOut,
///     retain: bool,
/// }
/// impl MqttIn {
///     fn new(payload: impl Into<MqttOut>, retain: bool) -> Self {
///         Self { payload: payload.into(), retain }
///     }
/// }
///
/// # settings! {
/// #     pub struct MQTTSettings in "mqtt_client" {
/// #         host: String = "localhost".into(),
/// #         port: u16 = 1883,
/// #         client_id: String = "p1".into(),
/// #     }
/// #     is_valid |s| {
/// #         if s.client_id.is_empty() || s.host.is_empty() {
/// #             return false;
/// #         }
/// #         true
/// #     }
/// # }
/// # pub struct MQTTStream(broadcast::Receiver<MqttOut>);
/// # impl ProtoStream for MQTTStream {
/// #     type Out = MqttOut;
/// #     fn recv(&mut self) -> PinBoxFuture<'_, Option<Self::Out>> {
/// #         Box::pin(async move {
/// #             loop {
/// #                 match self.0.recv().await {
/// #                     Ok(v) => return Some(v),
/// #                     Err(RecvError::Lagged(n)) => tracing::warn!("MqttListen: lagged {n}"),
/// #                     Err(RecvError::Closed) => return None,
/// #                 }
/// #             }
/// #         })
/// #     }
/// # }
/// handle! {
///     pub struct MQTTHandle {
///         client: Arc<AsyncClient>,
///         subscriptions: Arc<DashMap<String, broadcast::Sender<MqttOut>>>,
///     }
/// }
/// # impl MQTTHandle {
/// #     fn parse_qos(qos: u8) -> Result<rumqttc::QoS, ProtoError> {
/// #         match qos {
/// #             0 => Ok(rumqttc::QoS::AtMostOnce),
/// #             _ => Err(ProtoError::Config(format!("{qos} not a valid QoS"))),
/// #         }
/// #     }
/// # }
/// impl ProtoHandle for MQTTHandle {
///     type Settings = MQTTSettings;
///     type NodeConfig = u8;
///     type In = MqttIn;
///     type Out = MqttOut;
///
///     fn connect(settings: MQTTSettings, runtime: Handle) -> Result<Self, ProtoError> {
///         // ...
///         # let MQTTSettings {
///         #     client_id,
///         #     host,
///         #     port,
///         # } = settings;
///         # let mut mqttoptions = MqttOptions::new(client_id, host, port);
///         # let (client, mut _eventloop) = AsyncClient::new(mqttoptions, 64);
///         # let subscriptions: Arc<DashMap<String, broadcast::Sender<MqttOut>>> = Arc::new(DashMap::new());
///         # Ok(Self {
///         #     client: Arc::new(client),
///         #     subscriptions,
///         # })
///     }
///
///     fn publish(
///         &self,
///         topic: &str,
///         payload: MqttIn,
///         qos: Self::NodeConfig,
///     ) -> ProtoFuture<'_, ()> {
///         // ...
///         # let topic = topic.to_string();
///         # Box::pin(async move {
///         #     self.client
///         #         .publish(&topic, Self::parse_qos(qos)?, payload.retain, payload.payload)
///         #         .await
///         #         .map_err(|e| ProtoError::Publish(format!("Failed to publish to {topic} topic: {e}")))?;
///         #     Ok(())
///         # })
///     }
///
///     fn subscribe(
///         &self,
///         topic: &str,
///         qos: Self::NodeConfig,
///     ) -> ProtoFuture<'_, Box<dyn ProtoStream<Out = MqttOut>>> {
///         // ...
///         # let topic = topic.to_string();
///         # Box::pin(async move {
///         #     if let Some(tx) = self.subscriptions.get(&topic) {
///         #         return Ok(Box::new(MQTTStream(tx.subscribe())) as Box<dyn ProtoStream<Out = MqttOut>>);
///         #     }
///         #     let (tx, rx) = broadcast::channel(16);
///         #     self.client
///         #         .subscribe(&topic, Self::parse_qos(qos)?)
///         #         .await
///         #         .map_err(|e| {
///         #             ProtoError::Subscribe(format!("Failed to subscribe to {topic} topic: {e}"))
///         #         })?;
///         #     self.subscriptions.insert(topic, tx);
///         #     Ok(Box::new(MQTTStream(rx)) as Box<dyn ProtoStream<Out = MqttOut>>)
///         # })
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

/// [`type Out`][ProtoStream::Out]: recommend [`Vec<u8>`] or [`serde_json::Value`]. Any other should be a custom type.
///
/// # Example
/// ```
/// # #[macro_use] extern crate rmf2_task_orchestrator;
/// # use rmf2_task_orchestrator::client::protocol::*;
/// # use tokio::sync::broadcast;
/// # use tokio::sync::broadcast::error::RecvError;
/// pub type MqttOut = Vec<u8>;
///
/// pub struct MQTTStream(broadcast::Receiver<MqttOut>);
/// impl ProtoStream for MQTTStream {
///     type Out = MqttOut;
///     fn recv(&mut self) -> PinBoxFuture<'_, Option<Self::Out>> {
///         Box::pin(async move {
///             loop {
///                 match self.0.recv().await {
///                     Ok(v) => return Some(v),
///                     Err(RecvError::Lagged(n)) => tracing::warn!("Mqtt: lagged {n}"),
///                     Err(RecvError::Closed) => return None,
///                 }
///             }
///         })
///     }
/// }
/// ```
pub trait ProtoStream: Send + 'static {
    type Out;
    fn recv(&mut self) -> PinBoxFuture<'_, Option<Self::Out>>;
}

// -----------------------------------------------------------------

/// Wrapper to initialise [`ProtoHandle`] from the given [`ProtoSettings`].
pub struct EnsureProto<Handle: ProtoHandle>(Arc<Mutex<Option<Handle::Settings>>>);

impl<Handle: ProtoHandle> EnsureProto<Handle> {
    pub fn new(settings: Option<Handle::Settings>) -> Self {
        Self(Arc::new(Mutex::new(Some(settings.unwrap_or_else(|| {
            Handle::Settings::load_config().unwrap()
        })))))
    }
}

// Manually implementing #[derive(Clone)]
impl<Handle: ProtoHandle> Clone for EnsureProto<Handle> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<Handle: ProtoHandle> bevy_ecs::system::Command for EnsureProto<Handle> {
    fn apply(self, world: &mut bevy_ecs::prelude::World) {
        if let Some(config) = (self.0).lock().unwrap().take() {
            let runtime = world.resource::<crate::TokioHandle>().0.clone();
            let instance = Handle::connect(config, runtime)
                .unwrap_or_else(|e| panic!("Failed to connect {}: {e}", get_type::<Handle>()));
            world.insert_resource(instance);
        }
    }
}
