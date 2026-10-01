//! Cœur de sûreté de Vigie.
//!
//! Tout ce qui décide d'une action sur le drone vit ici : pas d'I/O, pas d'allocation,
//! pas d'horloge système. Le temps est passé en paramètre (`now_ms`), ce qui rend
//! chaque décision rejouable dans un test.
//!
//! La crate compile en `no_std` pour pouvoir, un jour, tourner sur un MCU de
//! supervision indépendant du companion computer. La CI le vérifie en compilant
//! pour `thumbv7em-none-eabihf`.

#![no_std]

#[cfg(test)]
extern crate std;

pub mod failsafe;
pub mod geo;
pub mod geofence;

pub use failsafe::{Action, FailsafeConfig, FailsafeMonitor, Inputs, Reason, State};
pub use geo::{GeoPoint, LocalFrame, Vec2};
pub use geofence::{FenceBreach, FenceError, FenceStatus, Geofence};
