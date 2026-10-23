use std::{
    collections::HashSet,
    env,
    error::Error,
    future::Future,
    sync::{Arc, Mutex},
    task::{Context, Poll, Wake, Waker},
    thread,
    time::Duration,
};
use zbus::export::ordered_stream::OrderedStreamExt;
use zbus::{connection::Builder, interface, object_server::SignalEmitter};

const BUS_NAME: &str = "com.canonical.Myna.Dictation";
const OBJECT_PATH: &str = "/com/canonical/Myna/Dictation";

#[derive(Clone)]
struct State {
    state: String,
    status: String,
    rms: f64,
    peak: f64,
    last_error: String,
    last_error_detail: String,
    clients: HashSet<String>,
    toggle_calls: u32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            status: String::new(),
            rms: 0.0,
            peak: 0.0,
            last_error: String::new(),
            last_error_detail: String::new(),
            clients: HashSet::new(),
            toggle_calls: 0,
        }
    }
}

struct Myna(Arc<Mutex<State>>);

#[interface(name = "com.canonical.Myna.Dictation")]
impl Myna {
    #[zbus(property)]
    fn state(&self) -> String {
        self.0.lock().unwrap().state.clone()
    }
    #[zbus(property)]
    fn status_message(&self) -> String {
        self.0.lock().unwrap().status.clone()
    }
    #[zbus(property)]
    fn audio_rms(&self) -> f64 {
        self.0.lock().unwrap().rms
    }
    #[zbus(property)]
    fn audio_peak(&self) -> f64 {
        self.0.lock().unwrap().peak
    }
    #[zbus(property)]
    fn last_error(&self) -> String {
        self.0.lock().unwrap().last_error.clone()
    }
    #[zbus(property)]
    fn last_error_detail(&self) -> String {
        self.0.lock().unwrap().last_error_detail.clone()
    }
    #[zbus(property)]
    fn hud_style(&self) -> String {
        "vumeter".into()
    }
    #[zbus(property)]
    fn activation(&self) -> String {
        "control".into()
    }
    fn toggle(&self) {
        self.0.lock().unwrap().toggle_calls += 1;
    }
    fn preview_sounds(&self, _sounds: Vec<String>) {}
    fn register_client(&self, #[zbus(header)] header: zbus::message::Header<'_>) -> u32 {
        let sender = header.sender().map(|s| s.to_string()).unwrap_or_default();
        let mut state = self.0.lock().unwrap();
        state.clients.insert(sender);
        state.clients.len() as u32
    }
    fn unregister_client(&self, #[zbus(header)] header: zbus::message::Header<'_>) -> u32 {
        let sender = header.sender().map(|s| s.to_string()).unwrap_or_default();
        let mut state = self.0.lock().unwrap();
        state.clients.remove(&sender);
        state.clients.len() as u32
    }
}

struct Control(Arc<Mutex<State>>);
#[interface(name = "org.mynalayout.FakeControl")]
impl Control {
    #[zbus(property)]
    fn registered_clients(&self) -> u32 {
        self.0.lock().unwrap().clients.len() as u32
    }
    #[zbus(property)]
    fn toggle_calls(&self) -> u32 {
        self.0.lock().unwrap().toggle_calls
    }
    #[zbus(property)]
    fn toast_suppressed(&self) -> bool {
        !self.0.lock().unwrap().clients.is_empty()
    }
    async fn set_state(
        &self,
        state: String,
        text: String,
        level: f64,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        // Any non-empty name is accepted on purpose: tests need to publish a state the
        // indicator does not know (RN-NOVA-1).
        if state.is_empty() {
            return Err(zbus::fdo::Error::InvalidArgs("empty state".into()));
        }
        {
            let mut s = self.0.lock().unwrap();
            s.state = state.clone();
            s.status = text.clone();
            s.rms = if state == "recording" {
                level.clamp(0.0, 1.0)
            } else {
                0.0
            };
            s.peak = s.rms;
            s.last_error = if state == "error" {
                text.clone()
            } else {
                String::new()
            };
            s.last_error_detail = s.last_error.clone();
        }
        let snapshot = self.0.lock().unwrap().clone();
        let mut changed = std::collections::HashMap::new();
        changed.insert("State", zbus::zvariant::Variant::new(snapshot.state));
        changed.insert(
            "StatusMessage",
            zbus::zvariant::Variant::new(snapshot.status),
        );
        changed.insert("AudioRms", zbus::zvariant::Variant::new(snapshot.rms));
        changed.insert("AudioPeak", zbus::zvariant::Variant::new(snapshot.peak));
        changed.insert(
            "LastError",
            zbus::zvariant::Variant::new(snapshot.last_error),
        );
        changed.insert(
            "LastErrorDetail",
            zbus::zvariant::Variant::new(snapshot.last_error_detail),
        );
        zbus::fdo::Properties::properties_changed(
            &emitter,
            "com.canonical.Myna.Dictation".try_into().unwrap(),
            changed,
            std::borrow::Cow::Borrowed(&[]),
        )
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }
    async fn set_level(
        &self,
        level: f64,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let (rms, peak, recording) = {
            let mut s = self.0.lock().unwrap();
            if s.state == "recording" {
                s.rms = level.clamp(0.0, 1.0);
                s.peak = s.rms;
            }
            (s.rms, s.peak, s.state == "recording")
        };
        let mut changed = std::collections::HashMap::new();
        if recording {
            changed.insert("AudioRms", zbus::zvariant::Variant::new(rms));
            changed.insert("AudioPeak", zbus::zvariant::Variant::new(peak));
        }
        zbus::fdo::Properties::properties_changed(
            &emitter,
            "com.canonical.Myna.Dictation".try_into().unwrap(),
            changed,
            std::borrow::Cow::Borrowed(&[]),
        )
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn restart(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let bus = zbus::fdo::DBusProxy::new(connection)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        bus.release_name(
            BUS_NAME
                .try_into()
                .map_err(|e: zbus::Error| zbus::fdo::Error::Failed(e.to_string()))?,
        )
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        thread::sleep(Duration::from_millis(300));
        *self.0.lock().unwrap() = State::default();
        bus.request_name(
            BUS_NAME
                .try_into()
                .map_err(|e: zbus::Error| zbus::fdo::Error::Failed(e.to_string()))?,
            zbus::fdo::RequestNameFlags::DO_NOT_QUEUE,
        )
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        let mut changed = std::collections::HashMap::new();
        changed.insert("State", zbus::zvariant::Variant::new("idle"));
        changed.insert("StatusMessage", zbus::zvariant::Variant::new(""));
        changed.insert("AudioRms", zbus::zvariant::Variant::new(0.0_f64));
        changed.insert("AudioPeak", zbus::zvariant::Variant::new(0.0_f64));
        changed.insert("LastError", zbus::zvariant::Variant::new(""));
        changed.insert("LastErrorDetail", zbus::zvariant::Variant::new(""));
        zbus::fdo::Properties::properties_changed(
            &emitter,
            "com.canonical.Myna.Dictation".try_into().unwrap(),
            changed,
            std::borrow::Cow::Borrowed(&[]),
        )
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }
    fn quit(&self) {
        std::process::exit(0);
    }
}

struct ThreadWake(thread::Thread);
impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}
fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    block_on(run())
}
async fn run() -> Result<(), Box<dyn Error>> {
    let address = env::var("FAKE_MYNA_BUS_ADDRESS")
        .map_err(|_| "private bus address required via FAKE_MYNA_BUS_ADDRESS")?;
    if env::var("DBUS_SESSION_BUS_ADDRESS").ok().as_deref() == Some(address.as_str()) {
        return Err("refusing the inherited session bus; provide a private test bus".into());
    }
    let state = Arc::new(Mutex::new(State::default()));
    let connection = Builder::address(address.as_str())
        .name(BUS_NAME)
        .serve_at(OBJECT_PATH, Myna(state.clone()))
        .serve_at(OBJECT_PATH, Control(state.clone()))
        .build()
        .await?;
    let _connection = connection;
    let dbus = zbus::fdo::DBusProxy::new(&_connection).await?;
    let mut owner_changes = dbus.receive_name_owner_changed().await?;
    let clients = state.clone();
    thread::spawn(move || {
        loop {
            let Some(change) = block_on(owner_changes.next()) else {
                break;
            };
            if let Ok(args) = change.args() {
                let name = args.name().as_str().to_owned();
                if name.starts_with(':') && args.old_owner().is_some() && args.new_owner().is_none()
                {
                    clients.lock().unwrap().clients.remove(name.as_str());
                }
            }
        }
    });
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
