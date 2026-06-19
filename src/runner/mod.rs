#![allow(dead_code, unused_imports)]

mod cli;
pub(crate) mod event_loop;
mod fixtures;
mod images;
pub(crate) mod pipeline;
pub(crate) mod scripts;

pub(crate) use cli::CliOptions;
pub(crate) use images::{
    ImageCache, SvgCache, load_images, load_missing_images, load_missing_svgs, load_svgs,
};
pub(crate) use pipeline::{fetch_script, run_browser};
