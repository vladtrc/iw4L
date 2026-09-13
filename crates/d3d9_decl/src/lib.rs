#![no_std]
#![forbid(unsafe_code)]

pub mod decl_type;
pub mod usage;

pub use decl_type::DeclType;
pub use usage::{
    D3DDECLUSAGE_COLOR, D3DDECLUSAGE_DEPTH, D3DDECLUSAGE_NORMAL, D3DDECLUSAGE_POSITION,
    D3DDECLUSAGE_TEXCOORD, Semantic,
};
