//! The client: one worker thread owns the D-Bus connection and turns what happens on
//! the bus into events.

use std::{collections::HashMap, fmt, thread};

use futures_lite::{StreamExt, future};
use zbus::{
    Connection,
    fdo::{DBusProxy, PropertiesChangedStream, PropertiesProxy},
    names::InterfaceName,
};

use super::protocol::{self, BUS_NAME, INTERFACE, OBJECT_PATH, Properties};
use crate::core::{Event, EventSink, MynaClient};

/// Why the client could not start.
#[derive(Debug)]
pub struct MynaError(String);

impl fmt::Display for MynaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cannot reach the session bus: {}", self.0)
    }
}

impl std::error::Error for MynaError {}

/// A request from the event loop to the worker.
enum Request {
    Register,
    Unregister,
}

/// The client of Myna's D-Bus interface.
pub struct DbusMynaClient {
    requests: async_channel::Sender<Request>,
    version: Option<String>,
}

impl DbusMynaClient {
    /// Connect and start watching. `bus_address` is `None` for the user's session bus
    /// and an address for a private bus (tests). Presence and readings arrive through
    /// `sink`; this call does not block.
    pub fn connect(bus_address: Option<&str>, sink: EventSink) -> Result<Self, MynaError> {
        let address = bus_address.map(str::to_owned);
        let connection = zbus::block_on(async {
            match &address {
                Some(address) => {
                    zbus::connection::Builder::address(address.as_str())
                        .build()
                        .await
                }
                None => zbus::connection::Builder::session().build().await,
            }
        })
        .map_err(|error| MynaError(error.to_string()))?;
        let (requests, receiver) = async_channel::unbounded();
        thread::Builder::new()
            .name("myna-client".into())
            .spawn(move || {
                if let Err(error) = zbus::block_on(watch(connection, sink, receiver)) {
                    tracing::error!(%error, "the Myna client stopped");
                }
            })
            .map_err(|error| MynaError(error.to_string()))?;
        Ok(Self {
            requests,
            version: protocol::installed_version(),
        })
    }
}

impl MynaClient for DbusMynaClient {
    fn register(&mut self) {
        let _ = self.requests.try_send(Request::Register);
    }

    fn unregister(&mut self) {
        let _ = self.requests.try_send(Request::Unregister);
    }

    fn version(&self) -> Option<String> {
        self.version.clone()
    }
}

/// What the worker waits for.
enum Input {
    Owner(Option<bool>),
    Request(Option<Request>),
    Changed(Option<HashMap<String, Value>>),
}

type Value = zbus::OwnedVariant;

async fn watch(
    connection: Connection,
    sink: EventSink,
    requests: async_channel::Receiver<Request>,
) -> zbus::Result<()> {
    let dbus = DBusProxy::new(&connection).await?;
    let mut owner_changes = dbus.receive_name_owner_changed().await?;
    let mut properties_stream = None;
    let mut cache = Properties::default();

    if dbus.name_has_owner(BUS_NAME.try_into()?).await? {
        properties_stream = attach(&connection, &sink, &mut cache).await;
    } else {
        sink(Event::MynaAbsent);
    }

    loop {
        let owner = async {
            let change = owner_changes.next().await;
            Input::Owner(change.and_then(|signal| {
                let args = signal.args().ok()?;
                (args.name().as_str() == BUS_NAME).then(|| args.new_owner().is_some())
            }))
        };
        let request = async { Input::Request(requests.recv().await.ok()) };
        let changed = async {
            match properties_stream.as_mut() {
                Some(stream) => Input::Changed(next_changes(stream).await),
                None => future::pending().await,
            }
        };
        match future::or(owner, future::or(request, changed)).await {
            Input::Owner(Some(true)) => {
                properties_stream = attach(&connection, &sink, &mut cache).await;
            }
            Input::Owner(Some(false)) => {
                properties_stream = None;
                cache = Properties::default();
                sink(Event::MynaAbsent);
            }
            Input::Owner(None) => {}
            Input::Request(Some(request)) => call_registration(&connection, request).await,
            Input::Request(None) => return Ok(()),
            Input::Changed(Some(changes)) => {
                merge(&mut cache, &changes);
                sink(Event::Myna(protocol::reading(&cache)));
            }
            Input::Changed(None) => properties_stream = None,
        }
    }
}

/// Myna just appeared: read its properties, check them and report the first reading
/// before the presence, so the `core` already knows the state when it registers.
async fn attach(
    connection: &Connection,
    sink: &EventSink,
    cache: &mut Properties,
) -> Option<PropertiesChangedStream> {
    let properties = match PropertiesProxy::builder(connection)
        .destination(BUS_NAME)
        .path(OBJECT_PATH)
        .build()
        .await
    {
        Ok(proxy) => proxy,
        Err(error) => {
            tracing::warn!(%error, "cannot build the properties proxy");
            sink(Event::MynaIncompatible);
            return None;
        }
    };
    let all = match properties
        .get_all(InterfaceName::try_from(INTERFACE).ok()?)
        .await
    {
        Ok(all) => all,
        Err(error) => {
            tracing::warn!(%error, "cannot read Myna's properties");
            sink(Event::MynaIncompatible);
            return None;
        }
    };
    *cache = Properties::default();
    merge(cache, &all.into_iter().collect());
    if let Err(reason) = protocol::check_compatible(cache) {
        tracing::warn!(%reason, version = ?protocol::installed_version(), "Myna's interface is not the expected one");
        sink(Event::MynaIncompatible);
        return None;
    }
    tracing::info!(version = ?protocol::installed_version(), "observing Myna");
    sink(Event::Myna(protocol::reading(cache)));
    sink(Event::MynaPresent);
    properties.receive_properties_changed().await.ok()
}

async fn next_changes(stream: &mut PropertiesChangedStream) -> Option<HashMap<String, Value>> {
    loop {
        let signal = stream.next().await?;
        let args = signal.args().ok()?;
        if args.interface_name().as_str() != INTERFACE {
            continue;
        }
        let changes = args
            .changed_properties()
            .iter()
            .filter_map(|(name, value)| Some(((*name).to_owned(), value.try_to_owned().ok()?)))
            .collect();
        return Some(changes);
    }
}

fn merge(cache: &mut Properties, changes: &HashMap<String, Value>) {
    if let Some(value) = changes.get("State") {
        cache.state = text(value);
    }
    if let Some(value) = changes.get("StatusMessage") {
        cache.status_message = text(value);
    }
    if let Some(value) = changes.get("AudioRms") {
        cache.audio_rms = f64::try_from(value).ok();
    }
    if let Some(value) = changes.get("LastError") {
        cache.last_error = text(value);
    }
}

fn text(value: &Value) -> Option<String> {
    <&str>::try_from(value).ok().map(str::to_owned)
}

async fn call_registration(connection: &Connection, request: Request) {
    let method = match request {
        Request::Register => "RegisterClient",
        Request::Unregister => "UnregisterClient",
    };
    let result = async {
        let proxy = zbus::Proxy::new(connection, BUS_NAME, OBJECT_PATH, INTERFACE).await?;
        proxy.call::<_, _, u32>(method, &()).await
    }
    .await;
    if let Err(error) = result {
        tracing::warn!(%error, method, "the registration call failed");
    }
}
