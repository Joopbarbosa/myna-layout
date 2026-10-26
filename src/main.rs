//! Composition root: wires the modules together without holding business rules.
//!
//! One event loop on the main thread joins everything (DT-NOVA-5): the Wayland
//! connection of the bar, and a channel that the D-Bus worker threads, the signal
//! thread and the bar itself feed with [`Event`] values. Every event goes through
//! [`runtime::step`], which asks the `core` what to do and does it.
//!
//! `MYNA_LAYOUT_BUS_ADDRESS` points the D-Bus modules at a private bus; it exists for
//! the tests and is never set in normal use.

use std::{collections::VecDeque, process::ExitCode, time::Duration};

use myna_layout::{
    config,
    core::{Bar, Event, EventSink, MynaClient, Notifier, State},
    i18n::Language,
    myna::DbusMynaClient,
    runtime::{self, Ports},
    surface::{
        bar::{BarState, Connected},
        notify::DbusNotifier,
    },
};
use smithay_client_toolkit::reexports::{
    calloop::{
        EventLoop, Interest, Mode, PostAction,
        channel::{self, Channel},
        generic::Generic,
    },
    client::{Connection, EventQueue, backend::WaylandError},
};

/// How long shutdown waits for the unregistration and the last close.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

/// Everything the event loop owns.
struct App {
    state: Option<State>,
    bar: Option<BarState>,
    queue: Option<EventQueue<BarState>>,
    connection: Option<Connection>,
    notifier: Option<DbusNotifier>,
    client: DbusMynaClient,
    done: bool,
}

impl App {
    fn ports(&mut self) -> Ports<'_> {
        Ports {
            bar: self.bar.as_mut().map(|bar| bar as &mut dyn Bar),
            notifier: self
                .notifier
                .as_mut()
                .map(|notifier| notifier as &mut dyn Notifier),
            client: Some(&mut self.client as &mut dyn MynaClient),
        }
    }

    /// Apply an event and everything it causes.
    fn handle(&mut self, event: Event) {
        if event == Event::Shutdown {
            self.done = true;
        }
        let mut pending = VecDeque::from([event]);
        while let Some(event) = pending.pop_front() {
            let state = self.state.take().expect("the state is always put back");
            let mut ports = self.ports();
            let (state, follow_ups) = runtime::step(state, event, &mut ports);
            self.state = Some(state);
            pending.extend(follow_ups);
        }
        self.flush_wayland();
    }

    /// Read what the compositor sent and let the bar react.
    fn dispatch_wayland(&mut self) {
        let (Some(connection), Some(queue), Some(bar)) = (
            self.connection.as_ref(),
            self.queue.as_mut(),
            self.bar.as_mut(),
        ) else {
            return;
        };
        let result = (|| {
            queue.dispatch_pending(bar)?;
            if let Some(guard) = connection.prepare_read() {
                match guard.read() {
                    // Nothing to read yet: the socket woke us spuriously.
                    Err(WaylandError::Io(io)) if io.kind() == std::io::ErrorKind::WouldBlock => {}
                    other => {
                        other?;
                    }
                }
            }
            queue.dispatch_pending(bar)?;
            Ok::<_, Box<dyn std::error::Error>>(())
        })();
        if let Err(error) = result {
            tracing::error!(%error, "the Wayland connection failed");
            self.bar = None;
            self.queue = None;
            self.connection = None;
            self.handle(Event::SurfaceFailed(myna_layout::core::Surface::Bar));
            return;
        }
        self.flush_wayland();
    }

    fn flush_wayland(&self) {
        if let Some(connection) = &self.connection
            && let Err(error) = connection.flush()
        {
            tracing::warn!(%error, "could not flush the Wayland connection");
        }
    }
}

fn sink(sender: &channel::Sender<Event>) -> EventSink {
    let sender = sender.clone();
    Box::new(move |event| {
        let _ = sender.send(event);
    })
}

fn main() -> ExitCode {
    tracing_subscriber::fmt::init();
    tracing::info!("myna-layout {} starting", env!("CARGO_PKG_VERSION"));
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "myna-layout stopped");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let loaded = config::load();
    for warning in &loaded.warnings {
        tracing::warn!("{warning}");
    }
    let config = loaded.config;
    let bus_address = std::env::var("MYNA_LAYOUT_BUS_ADDRESS").ok();

    let mut event_loop: EventLoop<App> = EventLoop::try_new()?;
    let handle = event_loop.handle();
    let (sender, channel): (channel::Sender<Event>, Channel<Event>) = channel::channel();
    handle
        .insert_source(channel, |event, _, app| {
            if let channel::Event::Msg(event) = event {
                app.handle(event);
            }
        })
        .map_err(|error| error.to_string())?;
    // The configuration is the first event, so the surfaces start before anything else.
    sender.send(Event::ConfigLoaded(config))?;

    // The signal thread turns a termination request into a shutdown event.
    {
        let sender = sender.clone();
        let mut signals = signal_hook::iterator::Signals::new([
            signal_hook::consts::SIGTERM,
            signal_hook::consts::SIGINT,
        ])?;
        std::thread::Builder::new()
            .name("signals".into())
            .spawn(move || {
                if signals.forever().next().is_some() {
                    let _ = sender.send(Event::Shutdown);
                }
            })?;
    }

    let client = DbusMynaClient::connect(bus_address.as_deref(), sink(&sender))?;
    let notifier =
        match DbusNotifier::connect(bus_address.as_deref(), Language::from_env(), sink(&sender)) {
            Ok(notifier) => Some(notifier),
            Err(error) => {
                tracing::warn!(%error, "no notification surface");
                None
            }
        };
    let (bar, queue, connection) = match BarState::connect(sink(&sender), config) {
        Ok(Connected {
            state,
            queue,
            connection,
        }) => {
            // Watch the Wayland socket in the same loop.
            let descriptor = connection.backend().poll_fd().try_clone_to_owned()?;
            handle
                .insert_source(
                    Generic::new(descriptor, Interest::READ, Mode::Level),
                    |_, _, app| {
                        app.dispatch_wayland();
                        Ok(PostAction::Continue)
                    },
                )
                .map_err(|error| error.to_string())?;
            (Some(state), Some(queue), Some(connection))
        }
        Err(error) => {
            tracing::warn!(%error, "no bar surface");
            (None, None, None)
        }
    };

    let mut app = App {
        state: Some(State::new(config)),
        bar,
        queue,
        connection,
        notifier,
        client,
        done: false,
    };
    let stop = event_loop.get_signal();
    event_loop.run(None, &mut app, |app| {
        if app.done {
            stop.stop();
        }
    })?;
    runtime::flush(&mut app.ports(), SHUTDOWN_GRACE);
    Ok(())
}
