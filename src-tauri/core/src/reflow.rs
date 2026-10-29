//! The reflowed reader's Rust side: the main file converted to HTML and taken
//! through the reader pipeline, one module per step. [`article`] lists the
//! steps in order.

pub mod article;
pub mod convert;
pub mod equations;
pub mod figures;
pub mod join;
pub mod sanitize;
