#![doc = include_str!("../README.md")]
#![deny(
    missing_debug_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications
)]

pub use async_trait::async_trait;
pub use instant;

pub mod deserialized_responses;
pub mod executor;
pub mod locks;
pub mod util;

#[cfg(target_arch = "wasm32")]
mod wasm_helpers;
#[cfg(target_arch = "wasm32")]
pub use wasm_helpers::SafeEncode;

/// Super trait that is used for our store traits, this trait will differ if
/// it's used on WASM. WASM targets will not require `Send` and `Sync` to have
/// implemented, while other targets will.
#[cfg(not(target_arch = "wasm32"))]
pub trait AsyncTraitDeps: std::fmt::Debug + Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: std::fmt::Debug + Send + Sync> AsyncTraitDeps for T {}

/// Super trait that is used for our store traits, this trait will differ if
/// it's used on WASM. WASM targets will not require `Send` and `Sync` to have
/// implemented, while other targets will.
#[cfg(target_arch = "wasm32")]
pub trait AsyncTraitDeps: std::fmt::Debug {}
#[cfg(target_arch = "wasm32")]
impl<T: std::fmt::Debug> AsyncTraitDeps for T {}
