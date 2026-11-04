//! L2 — the PortCode agent: config, permissions, tools, skills, MCP client, agent loop, sessions.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod agents;
pub mod config;
pub mod mcp;
pub mod permission;
pub mod run;
pub mod session;
pub mod skills;
pub mod tool;
pub mod tools;

pub use config::Config;
pub use permission::{AutoGate, Decision, Gate, Reply, Request, Rules};
pub use run::{DenyAsk, NullUi, Outcome, Runner, Status, Ui, UiEvent};
