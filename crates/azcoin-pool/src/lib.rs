//! AZCOIN pool library. Exposes service composition and the SV1 session handler.
//!
//! `composition` — Builds `PoolServices` from config (job source, share validator, share sink).
//! `sv1_handler` — `Sv1SessionHandler` implements `SessionEventHandler` for the SV1 protocol
//! layer, wiring authorize, notify registration, share processing, and block submission.

pub mod composition;
pub mod sv1_handler;
