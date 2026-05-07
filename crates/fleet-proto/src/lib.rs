//! Generated protobuf types for the Mine Fleet wire contract.
//!
//! The schema lives in `/proto/fleet.proto`; this crate is the only place
//! the generated code is exposed. Consumers depend on [`v1`] and never on
//! `prost_build` output paths directly, so the schema can evolve without
//! rippling through the workspace.

pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/fleet.v1.rs"));
}

pub use prost::Message;
