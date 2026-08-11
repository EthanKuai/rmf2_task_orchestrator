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

fn get_type<T: ?Sized>() -> &'static str {
    std::any::type_name::<T>()
}

// -----------------------------------------------------------------

// Send: Arc<Mutex<_>>
// DeserializeOwned + Default: load_base_configuration
/// A trait for protocol settings, providing methods for loading configuration.
pub trait ProtocolSettings: serde::de::DeserializeOwned + Default + Send {
    fn load_config() -> Self {
        crate::config::load_base_configuration::<Self>()
            .unwrap_or_else(|e| panic!("Failed to load {} config: {e}", get_type::<Self>()))
    }
    fn sanitise(self) -> Self;
}
