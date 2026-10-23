//! Throwaway feasibility check (task #1079): does the compositor accept a
//! layer-shell surface that never takes keyboard focus and lets clicks through?
//!
//! It lists the protocols the compositor advertises, shows a small bar for a few
//! seconds and reports whether the compositor configured it and whether our surface
//! ever received keyboard focus or a pointer event. Run: `cargo run --example spike_layer_shell`.
//! It is not part of the product and is deleted once the bar task has its own surface code.

use std::time::{Duration, Instant};

use smithay_client_toolkit::reexports::client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{PointerEvent, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};

const WIDTH: u32 = 200;
const HEIGHT: u32 = 40;
const SHOW_FOR: Duration = Duration::from_secs(8);

struct Spike {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    configured: Option<(u32, u32)>,
    closed_by_compositor: bool,
    keyboard_enter_on_our_surface: bool,
    pointer_event_on_our_surface: bool,
    drawn: bool,
}

fn main() {
    let conn = Connection::connect_to_env().expect("no Wayland display");
    let (globals, mut queue) = registry_queue_init(&conn).expect("registry");
    let qh = queue.handle();

    println!("== protocols advertised by the compositor (interesting ones)");
    globals.contents().with_list(|list| {
        for global in list {
            let name = global.interface.as_str();
            if name.contains("layer_shell")
                || name.contains("text_input")
                || name.contains("input_method")
                || name.contains("foreign_toplevel")
                || name.contains("cosmic")
                || name == "wl_seat"
                || name == "xdg_wm_base"
            {
                println!("  {name} v{}", global.version);
            }
        }
    });

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("layer shell is NOT available");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");

    let surface = compositor.create_surface(&qh);
    let layer =
        layer_shell.create_layer_surface(&qh, surface, Layer::Top, Some("myna-layout-spike"), None);
    layer.set_anchor(Anchor::BOTTOM);
    layer.set_size(WIDTH, HEIGHT);
    layer.set_margin(0, 0, 32, 0);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    // Click-through: an empty input region.
    let empty = Region::new(&compositor).expect("region");
    layer.wl_surface().set_input_region(Some(empty.wl_region()));
    layer.commit();

    let pool = SlotPool::new((WIDTH * HEIGHT * 4) as usize, &shm).expect("pool");
    let mut spike = Spike {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,
        pool,
        layer,
        keyboard: None,
        pointer: None,
        configured: None,
        closed_by_compositor: false,
        keyboard_enter_on_our_surface: false,
        pointer_event_on_our_surface: false,
        drawn: false,
    };

    let start = Instant::now();
    while start.elapsed() < SHOW_FOR && !spike.closed_by_compositor {
        queue.roundtrip(&mut spike).expect("dispatch");
        std::thread::sleep(Duration::from_millis(50));
    }

    println!("== result");
    println!("  configured by the compositor: {:?}", spike.configured);
    println!("  closed by the compositor: {}", spike.closed_by_compositor);
    println!("  buffer attached and shown: {}", spike.drawn);
    println!(
        "  our surface received keyboard focus: {}",
        spike.keyboard_enter_on_our_surface
    );
    println!(
        "  our surface received pointer events: {}",
        spike.pointer_event_on_our_surface
    );
}

impl Spike {
    fn draw(&mut self) {
        let stride = WIDTH as i32 * 4;
        let (buffer, canvas) = self
            .pool
            .create_buffer(
                WIDTH as i32,
                HEIGHT as i32,
                stride,
                wl_shm::Format::Argb8888,
            )
            .expect("buffer");
        let (pixels, _) = canvas.as_chunks_mut::<4>();
        for pixel in pixels {
            // Semi-opaque blue (premultiplied ARGB, little endian: B, G, R, A).
            *pixel = [0xB0, 0x60, 0x20, 0xC0];
        }
        let surface = self.layer.wl_surface();
        surface.damage_buffer(0, 0, WIDTH as i32, HEIGHT as i32);
        buffer.attach_to(surface).expect("attach");
        self.layer.commit();
        self.drawn = true;
    }
}

impl LayerShellHandler for Spike {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.closed_by_compositor = true;
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        self.configured = Some(configure.new_size);
        if !self.drawn {
            self.draw();
        }
    }
}

impl CompositorHandler for Spike {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Spike {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl SeatHandler for Spike {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Spike {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        if self.layer.wl_surface() == surface {
            self.keyboard_enter_on_our_surface = true;
        }
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
    }
}

impl PointerHandler for Spike {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        if events
            .iter()
            .any(|event| &event.surface == self.layer.wl_surface())
        {
            self.pointer_event_on_our_surface = true;
        }
    }
}

impl ShmHandler for Spike {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_registry!(Spike);
smithay_client_toolkit::delegate_dispatch2!(Spike);

impl ProvidesRegistryState for Spike {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}
