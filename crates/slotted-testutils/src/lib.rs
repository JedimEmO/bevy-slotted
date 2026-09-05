//! Internal test doubles and builders for the slotted workspace.
//!
//! This crate is a dev-dependency of the other members and is never published.
//! It will hold the fake authorities (`RecordingAuthority`, `RejectingAuthority`),
//! inventory and menu builders, and the script conformance suite that both Lua
//! adapters must pass. Consumer-facing UI automation lives in `slotted-test`
//! instead, which is a normal published crate. See `docs/PLAN.md` sections 4.3
//! and 4.12.
