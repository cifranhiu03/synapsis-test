//! Library surface for integration tests. The binary in `main.rs` only
//! handles process concerns (logging, signals, bind addr); everything
//! interesting lives here.

pub mod app;
pub mod dto;
pub mod error;
pub mod handlers;
pub mod state;
