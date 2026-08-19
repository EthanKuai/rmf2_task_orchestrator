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

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
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
/// A trait for protocol settings, providing methods for loading configuration.
/// Implement with [`protocol::settings!`].
pub trait ProtocolSettings: serde::de::DeserializeOwned + Default + Send {
    const TOML_NAME: &'static str;

    fn load_config() -> Result<Self, ProtocolError> {
        match crate::config::load_configuration_section::<Self>(Self::TOML_NAME) {
            Ok(settings) => Ok(settings),
            Err(::config::ConfigError::NotFound(_)) => {
                tracing::warn!(
                    "No [{}] table in configuration, using defaults for {}",
                    Self::TOML_NAME,
                    get_type::<Self>()
                );
                Ok(Self::default())
            }
            Err(e) => Err(ProtocolError::Config(format!(
                "Failed to load [{}] into {}: {e}",
                Self::TOML_NAME,
                get_type::<Self>()
            ))),
        }
    }
    fn sanitise(self) -> Self;
}

#[doc(hidden)]
#[macro_export]
macro_rules! __protocol_settings {
    ($(#[$m:meta])* $v:vis struct $name:ident in $table:literal {
        $($(#[$fm:meta])* $f:ident : $t:ty = $d:expr),* $(,)?
    }) => {
        $crate::__paste::paste! {
            #[doc(hidden)]
            #[allow(non_camel_case_types, unused_imports)]
            use $crate::__serde as [<__task_orchestrator_serde_ $name>];

            $(#[$m])*
            #[derive($crate::__serde::Deserialize, Clone, PartialEq, Debug)]
            #[serde(crate = "__task_orchestrator_serde_" $name, default, deny_unknown_fields)]
            $v struct $name {
                $($(#[$fm])* pub $f: $t),*
            }
        }

        impl ::core::default::Default for $name {
            fn default() -> Self {
                Self { $($f: $d),* }
            }
        }

        impl $crate::client::protocol::ProtocolSettings for $name {
            const TOML_NAME: &'static str = $table;

            fn sanitise(mut self) -> Self {
                let d = Self::default();
                $(if self.$f == <$t as ::core::default::Default>::default() { self.$f = d.$f; })*
                self
            }
        }
    };
}

pub use __protocol_settings as settings;
