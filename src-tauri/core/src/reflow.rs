//! The reflowed reader's Rust side: the main file converted to HTML, its
//! figures made self-contained, its widgets joined to the manifest and
//! mounted, its live equations given the values the PDF typeset, and the
//! result sanitized (see [`article`] for the order).

pub mod article;
pub mod convert;
pub mod equations;
pub mod figures;
pub mod join;
pub mod sanitize;
