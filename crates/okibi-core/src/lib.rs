//! The planner.
//!
//! `plan` is a pure function of (demand digests, invalidation event, service
//! manifests). No network, no clock, no randomness — a deadline is an input,
//! not something read from the environment. The same inputs must produce the
//! same plan byte for byte, because a warm plan is a derived artifact that
//! gets stored, diffed and reviewed, and a plan that cannot be re-derived is
//! not reviewable.
//!
//! The planning itself is not here yet — see `spec/planner.md` for the
//! algorithm it owes. What is here is the digest it will read.

pub mod digest;
pub mod invalidation;
pub mod manifest;
pub mod plan;
pub mod pricing;

pub use digest::{DigestRecord, Kind, TopEntry};
pub use invalidation::{Axis, InvalidationEvent, Scope};
pub use manifest::{Epoch, ServiceManifest, ZoomSemantics};
pub use plan::{Entry, Lane, WarmPlan};
pub use pricing::PricingTable;
