//! The planner.
//!
//! `plan` is a pure function of (demand digests, invalidation event, service
//! manifests). No network, no clock, no randomness — a deadline is an input,
//! not something read from the environment. The same inputs must produce the
//! same plan byte for byte, because a warm plan is a derived artifact that
//! gets stored, diffed and reviewed, and a plan that cannot be re-derived is
//! not reviewable.
//!
//! Nothing here yet — see `spec/planner.md` for the algorithm this owes.
