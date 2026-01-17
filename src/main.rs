use clap::Parser;
use csscolorparser::Color;
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let args = Args::parse();

    debug!("Argument: {:?}", args);

    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let event_loop_proxy = event_loop.create_proxy();

    std::thread::spawn(move || {
        let frame_interval = args.interval as u64;
        let frame_count = 5;
        for i in 0..frame_count {
            let _ = event_loop_proxy.send_event(UserEvent::Frame(i));
            std::thread::sleep(std::time::Duration::from_millis(frame_interval));
        }
        let _ = event_loop_proxy.send_event(UserEvent::Close);
    });

    event_loop.set_control_flow(ControlFlow::Wait);

    let settings = args.create_settings();
    let mut app = App::new(settings);
    event_loop.run_app(&mut app).map_err(Into::into)
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Circle radius [px]
    #[arg(short, long, default_value_t = 200, value_parser = clap::value_parser!(u32).range(1..4000))]
    radius: u32,

    /// Line width [px]
    #[arg(short, long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..100))]
    line_width: u32,

    /// Line color (CSS color format)
    #[arg(short, long, default_value = "orangered", value_parser = csscolorparser::parse)]
    color: Color,

    /// Edge color (CSS color format)
    #[arg(short, long, default_value = "gray", value_parser = csscolorparser::parse)]
    edge_color: Color,

    /// Frame interval [ms]
    #[arg(short, long, default_value_t = 70)]
    interval: u32,
}

impl Args {
    fn color_to_argb(color: &Color) -> u32 {
        let [r, g, b, a] = color.to_rgba8();
        (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32
    }

    fn create_settings(&self) -> Settings {
        let color_argb = Self::color_to_argb(&self.color);
        let edge_color_argb = Self::color_to_argb(&self.edge_color);

        Settings::new(self.radius, self.line_width, color_argb, edge_color_argb)
    }
}

#[derive(Debug, Clone, Copy)]
enum UserEvent {
    Frame(u32),
    Close,
}

struct App {
    settings: Settings,
    draw_context: Option<DrawBuffer>,
}

impl App {
    fn new(settings: Settings) -> Self {
        Self {
            settings,
            draw_context: None,
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let win_size = (self.settings.radius() * 2) as i32;

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

        let window = event_loop.create_window(attr).unwrap();

        self.draw_context = Some(DrawBuffer::new(window));
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Frame(n) => {
                debug!("Frame {}", n);

                let current_radius = self.settings.radius() / (n + 1);

                self.draw_context.as_mut().unwrap().draw_circle(
                    current_radius,
                    self.settings.line_width(),
                    self.settings.color_argb(),
                    self.settings.edge_color_argb(),
                );
            }
            UserEvent::Close => {
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}
}

struct Settings {
    radius: u32,
    line_width: u32,
    color_argb: u32,
    edge_color_argb: u32,
}

impl Settings {
    fn new(radius: u32, line_width: u32, color_argb: u32, edge_color_argb: u32) -> Self {
        Self {
            radius,
            line_width,
            color_argb,
            edge_color_argb,
        }
    }

    fn radius(&self) -> u32 {
        self.radius
    }

    fn line_width(&self) -> u32 {
        self.line_width
    }

    fn color_argb(&self) -> u32 {
        self.color_argb
    }

    fn edge_color_argb(&self) -> u32 {
        self.edge_color_argb
    }
}

struct DrawBuffer {
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    _context: softbuffer::Context<Rc<Window>>,
}

impl DrawBuffer {
    fn new(window: Window) -> Self {
        let window = Rc::new(window);
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window).unwrap();

        Self {
            surface,
            _context: context,
        }
    }

    fn size(&self) -> (u32, u32) {
        let size = self.surface.window().inner_size();
        (size.width, size.height)
    }

    fn draw_circle(&mut self, radius: u32, line_width: u32, color_argb: u32, edge_color_argb: u32) {
        let (w, h) = self.size();

        self.surface
            .resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap())
            .unwrap();

        let mut buffer = self.surface.buffer_mut().unwrap();

        let center_x = w / 2;
        let center_y = h / 2;

        let radius_outer = radius;
        let radius_line_outer = radius_outer.saturating_sub(1);
        let radius_line_inner = radius_line_outer.saturating_sub(line_width);
        let radius_inner = radius_line_inner.saturating_sub(1);

        let radius_outer_sq = radius_outer.pow(2);
        let radius_line_outer_sq = radius_line_outer.pow(2);
        let radius_line_inner_sq = radius_line_inner.pow(2);
        let radius_inner_sq = radius_inner.pow(2);

        for y in 0..h {
            let idx_y = y * w;
            let dist_y = y.abs_diff(center_y).pow(2);
            for x in 0..w {
                let idx = (idx_y + x) as usize;
                let dist_x = x.abs_diff(center_x).pow(2);
                let dist_sq = dist_x + dist_y;

                // 0xAA RR GG BB
                buffer[idx] = if (radius_inner_sq <= dist_sq && dist_sq < radius_line_inner_sq)
                    || (radius_line_outer_sq < dist_sq && dist_sq <= radius_outer_sq)
                {
                    edge_color_argb
                } else if radius_line_inner_sq <= dist_sq && dist_sq <= radius_line_outer_sq {
                    color_argb
                } else {
                    0x00000000
                };
            }
        }

        buffer.present().unwrap();
    }
}
