use device_query::{DeviceQuery, DeviceState, MouseState};
use log::{debug, info};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::x11::WindowAttributesExtX11;
use winit::window::{Window, WindowId};

#[derive(Debug, Clone, Copy)]
enum UserEvent {
    Frame(u8),
    Close,
}

#[derive(Default)]
struct App {
    window: Option<Rc<Window>>,
    context: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let win_size = 300;

        let device_state = DeviceState::new();
        let mouse: MouseState = device_state.get_mouse();
        let cursor_position = mouse.coords;
        info!("Cursor Position: {:?}", cursor_position);

        let attr = Window::default_attributes()
            .with_transparent(true)
            .with_decorations(false)
            .with_inner_size(PhysicalSize::new(win_size, win_size))
            .with_position(PhysicalPosition::new(
                cursor_position.0 - win_size / 2,
                cursor_position.1 - win_size / 2,
            ))
            // X11
            .with_override_redirect(true);

        let window = Rc::new(event_loop.create_window(attr).unwrap());

        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();

        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Frame(n) => {
                debug!("Frame {}", n);

                let (w, h) = {
                    let size = self.window.as_ref().unwrap().inner_size();
                    (size.width, size.height)
                };
                self.surface
                    .as_mut()
                    .unwrap()
                    .resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap())
                    .unwrap();

                let mut buffer = self.surface.as_mut().unwrap().buffer_mut().unwrap();
                let center_x = (w / 2) as i32;
                let center_y = (h / 2) as i32;
                let radius = (std::cmp::min(w, h) / (2 + n as u32) - 10) as i32;

                for y in 0..h as i32 {
                    for x in 0..w as i32 {
                        let idx = (y * w as i32 + x) as usize;
                        let dist_sq = (x - center_x).pow(2) + (y - center_y).pow(2);

                        // 0xAA RR GG BB
                        if dist_sq < radius.pow(2) && dist_sq > (radius - 5).pow(2) {
                            buffer[idx] = 0xFFFF0000;
                        } else {
                            buffer[idx] = 0x00000000;
                        }
                    }
                }

                buffer.present().unwrap();
            }
            UserEvent::Close => {
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let event_loop_proxy = event_loop.create_proxy();

    std::thread::spawn(move || {
        let frame_duration = 70;
        let frame_count = 5;
        for i in 0..frame_count {
            let _ = event_loop_proxy.send_event(UserEvent::Frame(i));
            std::thread::sleep(std::time::Duration::from_millis(frame_duration));
        }
        let _ = event_loop_proxy.send_event(UserEvent::Close);
    });

    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::default();
    event_loop.run_app(&mut app).map_err(Into::into)
}
