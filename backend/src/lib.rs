//! Exchange backend. One crate, several processes: `api`, `worker`, `migrate`
//! and `openapi` under `src/bin` all build on this library (DESIGN.md §13.3).

pub mod config;
pub mod db;
pub mod domain;
pub mod error;
pub mod http;
pub mod telemetry;
