//! L1 — porting workbench: survey, parity, scaffold, terminal screenshots and the GitHub Pages site generator.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

pub mod parity;
pub mod scaffold;
pub mod shot;
pub mod site;
pub mod survey;
