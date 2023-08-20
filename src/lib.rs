//! # `TuiScope`
//!
//! Inspired by [telescope](https://github.com/nvim-telescope/telescope.nvim).
//!
//! A TUI fuzzy finder for rust apps. For example usage, see [examples](https://github.com/olidacombe/tuiscope/tree/main/examples).
#![deny(clippy::pedantic)]
#![allow(clippy::must_use_candidate, clippy::return_self_not_must_use)]

mod data;
mod highlight;
mod widget;

pub use data::FuzzyFinder;
pub use widget::FuzzyList;
