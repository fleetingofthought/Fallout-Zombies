//! TES4-family plugin (ESM/ESP) support for the Fallout: New Vegas reimplementation.
//!
//! Layers:
//! - [`raw`]: lossless container structure (records, groups, subrecords) and writer
//! - [`plugin`]: one file plus its TES4 header
//! - [`loadorder`]: master resolution, FormID remapping, overrides
//! - [`schema`]: record layouts loaded from JSON at runtime
//! - [`decode`]: schema-driven decoding and coverage accounting
//! - [`survey`]: observational statistics used to write and check schemas
//! - [`inventory`]: per-plugin record inventory (the Layer 1 parity scenario)

pub mod decode;
pub mod inventory;
pub mod loadorder;
pub mod plugin;
pub mod raw;
pub mod schema;
pub mod survey;
pub mod text_guess;

pub use loadorder::LoadOrder;
pub use plugin::{Plugin, PluginHeader};
pub use raw::{EsmError, Group, GroupHeader, Node, Record, RecordHeader, Subrecord};
pub use schema::Schema;
