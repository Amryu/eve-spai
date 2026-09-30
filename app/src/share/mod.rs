//! Wormhole data shared in end-to-end encrypted groups. The server stores ciphertext and decides
//! who may fetch it; only members hold the keys. The crypto, the op log and hole records live in
//! `spai_share`, which the web app shares.

pub mod client;
pub mod engine;
pub mod keys;
pub use spai_share::{crypto, hole, ops};
