use super::fixtures::fixture_url;
use std::env;

#[derive(Debug, Clone)]
pub(crate) struct CliOptions {
    pub(crate) input_url: Option<String>,
    pub(crate) debug_dom: bool,
    pub(crate) debug_style: bool,
    pub(crate) debug_layout: bool,
}

impl CliOptions {
    pub(crate) fn from_env() -> Self {
        let mut args = env::args().skip(1);
        let mut input_url = None;
        let mut debug_dom = env_flag("AURORA_DEBUG_DOM");
        let mut debug_style = env_flag("AURORA_DEBUG_STYLE");
        let mut debug_layout = env_flag("AURORA_DEBUG_LAYOUT");

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--fixture" => {
                    let Some(name) = args.next() else {
                        eprintln!("Missing fixture name after --fixture");
                        std::process::exit(2);
                    };
                    input_url = Some(fixture_url(&name));
                }
                "--debug-dom" => debug_dom = true,
                "--debug-style" => debug_style = true,
                "--debug-layout" => debug_layout = true,
                other if other.starts_with("--") => {
                    eprintln!("Unknown flag: {other}");
                    std::process::exit(2);
                }
                other => {
                    input_url = Some(other.to_string());
                }
            }
        }

        Self {
            input_url,
            debug_dom,
            debug_style,
            debug_layout,
        }
    }
}

pub(crate) fn env_f32(name: &str) -> Option<f32> {
    env::var(name).ok()?.parse::<f32>().ok()
}

fn env_flag(name: &str) -> bool {
    matches!(
        env::var(name).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}
