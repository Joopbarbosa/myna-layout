//! The floating bar: a layer-shell surface that never takes keyboard focus and lets
//! clicks through (RN-NOVA-12).
//!
//! [`BarState`] owns everything the Wayland side needs and implements
//! [`crate::core::Bar`]. It does not run its own loop: [`BarState::connect`] hands
//! back the connection and the event queue so the composition root can plug them
//! into the single event loop (DT-NOVA-5). Results reach the `core` as events through
//! the sink given at connection time: `SurfaceReady` when the compositor configures
//! the surface and `SurfaceFailed` when it closes it (RN-NOVA-10, RN-NOVA-18).

mod placement;
mod render;
mod text;
mod wave;

use std::{fmt, time::Instant};

use smithay_client_toolkit::reexports::client::{
    Connection, EventQueue, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_shm, wl_surface},
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData, Region},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};

pub use placement::{Placement, placement};
pub use render::{BarView, PlainRenderer, Renderer};
pub use wave::WaveRenderer;

use crate::{
    core::{Bar, BarLook, Config, Event, EventSink, Surface},
    i18n::Language,
};

/// Why the bar could not connect to the compositor.
#[derive(Debug)]
pub enum BarError {
    /// There is no Wayland display to connect to.
    NoDisplay(String),
    /// The compositor does not offer a protocol the bar needs.
    MissingProtocol(&'static str),
    /// The compositor answered with a protocol error while connecting.
    Protocol(String),
}

impl fmt::Display for BarError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDisplay(detail) => write!(formatter, "no Wayland display: {detail}"),
            Self::MissingProtocol(name) => write!(formatter, "the compositor lacks {name}"),
            Self::Protocol(detail) => write!(formatter, "Wayland protocol error: {detail}"),
        }
    }
}

impl std::error::Error for BarError {}

/// The result of connecting: the state plus the pieces the event loop needs.
pub struct Connected {
    /// The bar, to be kept as part of the loop's data.
    pub state: BarState,
    /// The Wayland event queue, to be driven by the event loop.
    pub queue: EventQueue<BarState>,
    /// The Wayland connection.
    pub connection: Connection,
}

/// The Wayland side of the bar.
pub struct BarState {
    sink: EventSink,
    config: Config,
    queue_handle: QueueHandle<BarState>,
    registry_state: RegistryState,
    output_state: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    pool: SlotPool,
    renderer: Box<dyn Renderer>,
    started: bool,
    panels: Vec<Panel>,
    ready_reported: bool,
    wanted: Option<BarView>,
    level: f32,
    shown_at: Option<Instant>,
}

/// The bar on one monitor. Every monitor gets its own, so the bar shows wherever
/// the user is looking (RN-NOVA-16).
struct Panel {
    /// `None` only for the fallback panel, when no monitor is known yet and the
    /// compositor picks one.
    output: Option<wl_output::WlOutput>,
    layer: LayerSurface,
    configured_size: Option<(u32, u32)>,
    mapped: bool,
    frame_pending: bool,
}

impl BarState {
    /// Connect to the compositor and bind what the bar needs. It does not create the
    /// surface yet: that is [`Bar::start`].
    pub fn connect(sink: EventSink, config: Config) -> Result<Connected, BarError> {
        let connection =
            Connection::connect_to_env().map_err(|error| BarError::NoDisplay(error.to_string()))?;
        let (globals, queue) = registry_queue_init(&connection)
            .map_err(|error| BarError::Protocol(error.to_string()))?;
        let queue_handle = queue.handle();
        let compositor = CompositorState::bind(&globals, &queue_handle)
            .map_err(|_| BarError::MissingProtocol("wl_compositor"))?;
        let layer_shell = LayerShell::bind(&globals, &queue_handle)
            .map_err(|_| BarError::MissingProtocol("zwlr_layer_shell_v1"))?;
        let shm =
            Shm::bind(&globals, &queue_handle).map_err(|_| BarError::MissingProtocol("wl_shm"))?;
        let pool = SlotPool::new(
            (config.size.width as usize) * (config.size.height as usize) * 4,
            &shm,
        )
        .map_err(|error| BarError::Protocol(error.to_string()))?;
        let state = Self {
            sink,
            config,
            queue_handle: queue_handle.clone(),
            registry_state: RegistryState::new(&globals),
            output_state: OutputState::new(&globals, &queue_handle),
            compositor,
            layer_shell,
            shm,
            pool,
            renderer: Box::new(WaveRenderer::new(
                config.reduced_motion,
                Language::from_env(),
            )),
            started: false,
            panels: Vec::new(),
            ready_reported: false,
            wanted: None,
            level: 0.0,
            shown_at: None,
        };
        Ok(Connected {
            state,
            queue,
            connection,
        })
    }

    /// Replace the renderer (the animated one replaces the plain fill).
    pub fn set_renderer(&mut self, renderer: Box<dyn Renderer>) {
        self.renderer = renderer;
    }

    fn size_of(&self, panel: &Panel) -> (u32, u32) {
        match panel.configured_size {
            Some((width, height)) if width > 0 && height > 0 => (width, height),
            _ => (self.config.size.width, self.config.size.height),
        }
    }

    /// Create the layer surface for one monitor (or for the compositor's choice).
    /// `None` means the surface could not be set up.
    fn create_panel(&mut self, output: Option<wl_output::WlOutput>) -> Option<Panel> {
        let surface = self.compositor.create_surface(&self.queue_handle);
        let layer = self.layer_shell.create_layer_surface(
            &self.queue_handle,
            surface,
            Layer::Overlay,
            Some("myna-layout"),
            output.as_ref(),
        );
        let placed = placement(&self.config.position);
        layer.set_anchor(placed.anchor);
        let (top, right, bottom, left) = placed.margin;
        layer.set_margin(top, right, bottom, left);
        layer.set_size(self.config.size.width, self.config.size.height);
        layer.set_exclusive_zone(0);
        // Never take keyboard focus: Myna types into the focused field (RN-NOVA-12).
        layer.set_keyboard_interactivity(KEYBOARD_INTERACTIVITY);
        // Let clicks through: an empty input region.
        match Region::new(&self.compositor) {
            Ok(region) => layer
                .wl_surface()
                .set_input_region(Some(region.wl_region())),
            Err(error) => {
                tracing::warn!(%error, "could not create the empty input region");
                return None;
            }
        }
        layer.commit();
        Some(Panel {
            output,
            layer,
            configured_size: None,
            mapped: false,
            frame_pending: false,
        })
    }

    /// Open the bar on a monitor, unless it already has one.
    fn add_output(&mut self, output: wl_output::WlOutput) {
        // The fallback panel (compositor's choice) gives way to real monitors.
        self.panels.retain(|panel| panel.output.is_some());
        if self
            .panels
            .iter()
            .any(|panel| panel.output.as_ref() == Some(&output))
        {
            return;
        }
        match self.create_panel(Some(output)) {
            Some(panel) => self.panels.push(panel),
            None => (self.sink)(Event::SurfaceFailed(Surface::Bar)),
        }
    }

    /// Draw what is wanted on every monitor and attach it, or detach the buffers when
    /// nothing is wanted, so the bar disappears (RN-NOVA-13).
    fn present(&mut self) {
        for index in 0..self.panels.len() {
            self.present_panel(index);
        }
    }

    fn present_panel(&mut self, index: usize) {
        let layer = self.panels[index].layer.clone();
        if self.panels[index].configured_size.is_none() {
            return;
        }
        let Some(mut view) = self.wanted.clone() else {
            if self.panels[index].mapped {
                layer.wl_surface().attach(None, 0, 0);
                layer.commit();
                self.panels[index].mapped = false;
            }
            return;
        };
        view.elapsed = self.shown_at.map(|at| at.elapsed()).unwrap_or_default();
        let (width, height) = self.size_of(&self.panels[index]);
        let stride = width as i32 * 4;
        let Ok((buffer, canvas)) = self.pool.create_buffer(
            width as i32,
            height as i32,
            stride,
            wl_shm::Format::Argb8888,
        ) else {
            tracing::warn!("could not allocate a buffer for the bar");
            (self.sink)(Event::SurfaceFailed(Surface::Bar));
            return;
        };
        self.renderer.render(canvas, width, height, &view);
        layer
            .wl_surface()
            .damage_buffer(0, 0, width as i32, height as i32);
        if buffer.attach_to(layer.wl_surface()).is_err() {
            tracing::warn!("could not attach the bar buffer");
            (self.sink)(Event::SurfaceFailed(Surface::Bar));
            return;
        }
        // Ask for the next frame so the animation keeps going while the bar is shown.
        if !self.panels[index].frame_pending {
            self.panels[index].frame_pending = true;
            layer.wl_surface().frame(
                &self.queue_handle,
                FrameCallbackData(layer.wl_surface().clone()),
            );
        }
        layer.commit();
        self.panels[index].mapped = true;
    }

    fn panel_of_layer(&self, layer: &LayerSurface) -> Option<usize> {
        self.panels.iter().position(|panel| &panel.layer == layer)
    }

    fn panel_of_surface(&self, surface: &wl_surface::WlSurface) -> Option<usize> {
        self.panels
            .iter()
            .position(|panel| panel.layer.wl_surface() == surface)
    }
}

/// The bar never asks for keyboard focus (RN-NOVA-12).
const KEYBOARD_INTERACTIVITY: KeyboardInteractivity = KeyboardInteractivity::None;

impl Bar for BarState {
    fn start(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        let outputs: Vec<_> = self.output_state.outputs().collect();
        if outputs.is_empty() {
            // No monitor is known yet: let the compositor pick until one is announced.
            match self.create_panel(None) {
                Some(panel) => self.panels.push(panel),
                None => (self.sink)(Event::SurfaceFailed(Surface::Bar)),
            }
            return;
        }
        for output in outputs {
            self.add_output(output);
        }
    }

    fn show(&mut self, look: &BarLook) {
        if self.wanted.is_none() {
            self.shown_at = Some(Instant::now());
        }
        self.wanted = Some(BarView {
            look: look.clone(),
            level: self.level,
            opacity: self.config.opacity,
            elapsed: Default::default(),
        });
        self.present();
    }

    fn update_level(&mut self, level: f32) {
        // Only record the level: the next frame draws it, so a burst of readings does
        // not cost a buffer each.
        self.level = level;
        if let Some(view) = self.wanted.as_mut() {
            view.level = level;
        }
    }

    fn hide(&mut self) {
        self.wanted = None;
        self.shown_at = None;
        self.present();
    }
}

impl LayerShellHandler for BarState {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        tracing::warn!("the compositor closed a bar surface");
        if let Some(index) = self.panel_of_layer(layer) {
            self.panels.remove(index);
        }
        if self.panels.is_empty() {
            self.started = false;
            self.ready_reported = false;
            (self.sink)(Event::SurfaceFailed(Surface::Bar));
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if let Some(index) = self.panel_of_layer(layer) {
            self.panels[index].configured_size = Some(configure.new_size);
            if !self.ready_reported {
                self.ready_reported = true;
                (self.sink)(Event::SurfaceReady(Surface::Bar));
            }
            self.present_panel(index);
        }
    }
}

impl CompositorHandler for BarState {
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
    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if let Some(index) = self.panel_of_surface(surface) {
            self.panels[index].frame_pending = false;
            if self.wanted.is_some() {
                self.present_panel(index);
            }
        }
    }
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

impl OutputHandler for BarState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        if self.started {
            self.add_output(output);
        }
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.panels
            .retain(|panel| panel.output.as_ref() != Some(&output));
    }
}

impl ShmHandler for BarState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_registry!(BarState);
smithay_client_toolkit::delegate_dispatch2!(BarState);

impl ProvidesRegistryState for BarState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

#[cfg(test)]
mod focus_tests {
    use super::*;

    #[test]
    fn cen_novo_15_the_bar_never_asks_for_keyboard_focus() {
        assert_eq!(KEYBOARD_INTERACTIVITY, KeyboardInteractivity::None);
    }
}
