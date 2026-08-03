#![allow(dead_code)]

pub mod assets;
pub mod auth;
pub mod dto;
pub mod gotemplate;
pub mod handlers;
pub mod json;
pub mod router;
pub mod state;

pub use router::run;
