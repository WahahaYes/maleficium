//! The reflowed reader's Rust side: the main file converted to HTML, its
//! figures made self-contained, its widgets joined to the manifest and
//! mounted, and the result sanitized (see [`article`] for the order).

pub mod article;
pub mod convert;
pub mod figures;
pub mod join;
pub mod sanitize;
