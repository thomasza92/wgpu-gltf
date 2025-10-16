mod app;
mod graphics;

use crate::{app::App, graphics::Graphics};
use winit::event_loop::{ControlFlow, EventLoop, DeviceEvents};

fn run_app(event_loop: EventLoop<Graphics>, mut app: App) {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error")).init();
    let _ = event_loop.run_app(&mut app);
}

fn main() {
    let event_loop = EventLoop::<Graphics>::with_user_event().build().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.listen_device_events(DeviceEvents::Always);

    let app = App::new(&event_loop);
    run_app(event_loop, app);
}
