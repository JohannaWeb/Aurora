use super::app::AuroraApp;
use super::input::WindowInput;
use super::screenshot::render_to_file;
use winit::event_loop::EventLoop;

pub fn open(input: WindowInput) -> Result<(), String> {
    if let Ok(path) = std::env::var("AURORA_SCREENSHOT") {
        render_to_file(&input, &path);
        return Ok(());
    }

    let event_loop =
        EventLoop::new().map_err(|error| format!("failed to create event loop: {error}"))?;
    let mut app = AuroraApp::new(input);
    event_loop
        .run_app(&mut app)
        .map_err(|error| format!("failed to run event loop: {error}"))
}
