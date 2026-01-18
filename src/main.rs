use clap::Parser;
use csscolorparser::Color;
use device_query::{DeviceQuery, DeviceState, MouseState};
use log::{debug, info};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};
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

    let settings = args.create_settings();
    let mut app = App::new(settings);

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut app).map_err(Into::into)
}

#[derive(Debug, Clone)]
enum Radius {
    Auto,
    Value(u32),
}

impl std::str::FromStr for Radius {
    type Err = <u32 as std::str::FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(Radius::Auto),
            _ => s.parse::<u32>().map(Radius::Value),
        }
    }
}

#[derive(Debug, Clone)]
enum LineWidth {
    Auto,
    Value(u32),
}

impl std::str::FromStr for LineWidth {
    type Err = <u32 as std::str::FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(LineWidth::Auto),
            _ => s.parse::<u32>().map(LineWidth::Value),
        }
    }
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Circle radius \[px\]
    #[arg(short, long, default_value = "auto")]
    radius: Radius,

    /// Line width \[px\]
    #[arg(short, long, default_value = "auto")]
    line_width: LineWidth,

    /// Line color (CSS color format)
    #[arg(short, long, default_value = "orangered", value_parser = csscolorparser::parse)]
    color: Color,

    /// Edge color (CSS color format)
    #[arg(short, long, default_value = "gray", value_parser = csscolorparser::parse)]
    edge_color: Color,

    /// Frame interval \[ms\]
    #[arg(short, long, default_value = "70", value_parser = Args::parse_millis)]
    interval: Duration,
}

impl Args {
    fn parse_millis(arg: &str) -> Result<Duration, <u64 as std::str::FromStr>::Err> {
        arg.parse::<u64>().map(Duration::from_millis)
    }

    fn color_to_argb(color: &Color) -> u32 {
        let [r, g, b, a] = color.to_rgba8();
        (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32
    }

    fn create_settings(&self) -> Settings {
        let color_argb = Self::color_to_argb(&self.color);
        let edge_color_argb = Self::color_to_argb(&self.edge_color);

        Settings::new(
            self.radius.clone(),
            self.line_width.clone(),
            color_argb,
            edge_color_argb,
            self.interval,
        )
    }
}

struct App {
    settings: Settings,
    window: Option<Rc<Window>>,
    draw_buffer: Option<DrawBuffer>,

    radius_value: u32,
    line_width_value: u32,

    update_count: u32,
    next_update: Instant,
}

impl App {
    fn new(settings: Settings) -> Self {
        Self {
            settings,
            window: None,
            draw_buffer: None,
            radius_value: 0,
            line_width_value: 0,
            update_count: 0,
            next_update: Instant::now(),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let monitor_size = if let Some(monitor) = event_loop.primary_monitor() {
            let s = monitor.size();
            Some((s.width, s.height))
        } else {
            None
        };

        let win_size = (self.settings.radius(monitor_size) * 2) as i32;

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

        self.draw_buffer = Some(DrawBuffer::new(window.clone()));
        self.window = Some(window);
        self.radius_value = self.settings.radius(monitor_size);
        self.line_width_value = self.settings.line_width(monitor_size);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();

        if now >= self.next_update {
            self.update_count += 1;

            if self.update_count > 4 {
                event_loop.exit();
                return;
            }

            if let Some(window) = &self.window {
                window.request_redraw();
            }

            self.next_update = now + *self.settings.interval();
        }

        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_update));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                debug!("Frame {}", self.update_count);

                let current_radius = self.radius_value / (self.update_count + 1);

                self.draw_buffer.as_mut().unwrap().draw_circle(
                    current_radius,
                    self.line_width_value,
                    self.settings.color_argb(),
                    self.settings.edge_color_argb(),
                );
            }
            _ => (),
        }
    }
}

struct Settings {
    radius: Radius,
    line_width: LineWidth,
    color_argb: u32,
    edge_color_argb: u32,
    interval: Duration,
}

impl Settings {
    fn new(
        radius: Radius,
        line_width: LineWidth,
        color_argb: u32,
        edge_color_argb: u32,
        interval: Duration,
    ) -> Self {
        Self {
            radius,
            line_width,
            color_argb,
            edge_color_argb,
            interval,
        }
    }

    fn radius(&self, monitor_size: Option<(u32, u32)>) -> u32 {
        match self.radius {
            Radius::Value(v) => v,
            Radius::Auto => {
                let s = if let Some((w, h)) = monitor_size {
                    std::cmp::max(w, h)
                } else {
                    1920 // Full HD
                };

                std::cmp::max(s / 20, 50)
            }
        }
    }

    fn line_width(&self, monitor_size: Option<(u32, u32)>) -> u32 {
        match self.line_width {
            LineWidth::Value(v) => v,
            LineWidth::Auto => {
                let s = if let Some((w, h)) = monitor_size {
                    std::cmp::max(w, h)
                } else {
                    1920 // Full HD
                };

                std::cmp::max(s / 800, 3)
            }
        }
    }

    fn color_argb(&self) -> u32 {
        self.color_argb
    }

    fn edge_color_argb(&self) -> u32 {
        self.edge_color_argb
    }

    fn interval(&self) -> &Duration {
        &self.interval
    }
}

struct DrawBuffer {
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    _context: softbuffer::Context<Rc<Window>>,
}

impl DrawBuffer {
    fn new(window: Rc<Window>) -> Self {
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window).unwrap();

        Self {
            surface,
            _context: context,
        }
    }

    fn window_size(&self) -> (u32, u32) {
        let size = self.surface.window().inner_size();
        (size.width, size.height)
    }

    fn draw_circle(&mut self, radius: u32, line_width: u32, color_argb: u32, edge_color_argb: u32) {
        debug!(
            "Draw circle: radius={}px, line_width={}px, color={:#x}, edge_color={:#x}",
            radius, line_width, color_argb, edge_color_argb
        );

        let (w, h) = self.window_size();

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
