//! Integration tests for the notification surface against a private mock service.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread::sleep,
    time::{Duration, Instant},
};

use myna_layout::{
    core::{Event, Message, Notifier, OneTimeNotice, Surface, Urgency},
    i18n::Language,
    surface::notify::DbusNotifier,
};
use zbus::OwnedVariant;

#[derive(Clone, Debug)]
struct Call {
    replaces_id: u32,
    summary: String,
    urgency: u8,
    transient: bool,
}

struct MockNotifications {
    calls: Arc<Mutex<Vec<Call>>>,
    closed: Arc<Mutex<Vec<u32>>>,
    next_id: Arc<Mutex<u32>>,
    honor_replaces_id: bool,
    fail_close: bool,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl MockNotifications {
    fn get_server_information(&self) -> (&str, &str, &str, &str) {
        ("Mock", "Myna Layout tests", "1", "1.2")
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        _app_name: String,
        replaces_id: u32,
        _app_icon: String,
        summary: String,
        _body: String,
        _actions: Vec<String>,
        hints: HashMap<String, OwnedVariant>,
        _expire_timeout: i32,
    ) -> u32 {
        let urgency = hints
            .get("urgency")
            .and_then(|value| u8::try_from(value).ok())
            .unwrap_or_default();
        let transient = hints
            .get("transient")
            .and_then(|value| bool::try_from(value).ok())
            .unwrap_or(false);
        self.calls.lock().unwrap().push(Call {
            replaces_id,
            summary,
            urgency,
            transient,
        });
        let mut next_id = self.next_id.lock().unwrap();
        if self.honor_replaces_id && replaces_id != 0 {
            replaces_id
        } else {
            *next_id += 1;
            *next_id
        }
    }

    fn close_notification(&self, id: u32) -> zbus::fdo::Result<()> {
        self.closed.lock().unwrap().push(id);
        if self.fail_close {
            Err(zbus::fdo::Error::Failed("configured close failure".into()))
        } else {
            Ok(())
        }
    }
}

struct PrivateNotifications {
    address: String,
    daemon: Child,
    _service: Option<zbus::Connection>,
    calls: Arc<Mutex<Vec<Call>>>,
    closed: Arc<Mutex<Vec<u32>>>,
}

impl PrivateNotifications {
    fn start() -> Self {
        Self::configured(true, true, false)
    }
    fn configured(with_service: bool, honor_replaces_id: bool, fail_close: bool) -> Self {
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon must start for notification integration tests");
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().expect("bus output"))
            .read_line(&mut address)
            .expect("bus address");
        let calls = Arc::default();
        let closed = Arc::default();
        let mock = MockNotifications {
            calls: Arc::clone(&calls),
            closed: Arc::clone(&closed),
            next_id: Arc::default(),
            honor_replaces_id,
            fail_close,
        };
        let service = with_service.then(|| {
            zbus::block_on(
                zbus::connection::Builder::address(address.trim())
                    .name("org.freedesktop.Notifications")
                    .serve_at("/org/freedesktop/Notifications", mock)
                    .build(),
            )
            .expect("mock notification service")
        });
        Self {
            address: address.trim().to_owned(),
            daemon,
            _service: service,
            calls,
            closed,
        }
    }
}

impl Drop for PrivateNotifications {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

type Collected = Arc<Mutex<Vec<Event>>>;

fn notifier(bus: &PrivateNotifications) -> (DbusNotifier, Collected) {
    let events: Collected = Arc::default();
    let sink = {
        let events = Arc::clone(&events);
        Box::new(move |event| events.lock().unwrap().push(event))
    };
    (
        DbusNotifier::connect(Some(&bus.address), Language::En, sink).expect("notifier connect"),
        events,
    )
}

fn wait_for(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        sleep(Duration::from_millis(20));
    }
}

#[test]
fn start_reports_ready_and_show_sends_translated_message_and_urgency() {
    let bus = PrivateNotifications::start();
    let (mut notifier, events) = notifier(&bus);
    notifier.start();
    wait_for("notification surface ready", || {
        events
            .lock()
            .unwrap()
            .contains(&Event::SurfaceReady(Surface::Notification))
    });
    notifier.show(&Message::Listening, Urgency::Critical);
    wait_for("notification call", || {
        !bus.calls.lock().unwrap().is_empty()
    });
    let calls = bus.calls.lock().unwrap();
    assert_eq!(calls[0].summary, "Listening");
    assert_eq!(calls[0].urgency, 2);
    assert!(!calls.iter().any(|call| call.urgency == 0));
    assert!(calls[0].transient);
    assert_eq!(calls[0].replaces_id, 0);
}

#[test]
fn start_reports_failure_without_notification_service() {
    let bus = PrivateNotifications::configured(false, true, false);
    let (mut notifier, events) = notifier(&bus);
    notifier.start();
    wait_for("notification surface failure", || {
        events
            .lock()
            .unwrap()
            .contains(&Event::SurfaceFailed(Surface::Notification))
    });
    assert!(
        !events
            .lock()
            .unwrap()
            .contains(&Event::SurfaceReady(Surface::Notification))
    );
}

#[test]
fn repeated_shows_replace_without_closing_when_server_honors_id() {
    let bus = PrivateNotifications::start();
    let (mut notifier, _) = notifier(&bus);
    for _ in 0..3 {
        notifier.show(&Message::Listening, Urgency::Normal);
    }
    wait_for("three notification calls", || {
        bus.calls.lock().unwrap().len() == 3
    });
    notifier.flush(Duration::from_secs(2));
    assert!(bus.closed.lock().unwrap().is_empty());
    assert_eq!(
        bus.calls
            .lock()
            .unwrap()
            .iter()
            .map(|c| c.urgency)
            .collect::<Vec<_>>(),
        vec![1, 1, 1]
    );
}

#[test]
fn repeated_shows_close_old_ids_when_server_ignores_id() {
    let bus = PrivateNotifications::configured(true, false, false);
    let (mut notifier, _) = notifier(&bus);
    for _ in 0..3 {
        notifier.show(&Message::Listening, Urgency::Normal);
    }
    wait_for("three notification calls", || {
        bus.calls.lock().unwrap().len() == 3
    });
    notifier.flush(Duration::from_secs(2));
    assert_eq!(*bus.closed.lock().unwrap(), vec![1, 2]);
}

#[test]
fn close_failure_does_not_fail_notification_surface() {
    let bus = PrivateNotifications::configured(true, true, true);
    let (mut notifier, events) = notifier(&bus);
    notifier.show(&Message::Listening, Urgency::Normal);
    wait_for("notification call", || {
        !bus.calls.lock().unwrap().is_empty()
    });
    notifier.close();
    notifier.flush(Duration::from_secs(2));
    assert!(
        !events
            .lock()
            .unwrap()
            .contains(&Event::SurfaceFailed(Surface::Notification))
    );
}

#[test]
fn show_replaces_the_previous_notification_and_close_closes_it() {
    let bus = PrivateNotifications::start();
    let (mut notifier, _) = notifier(&bus);
    notifier.show(&Message::Loading, Urgency::Normal);
    wait_for("first notification", || {
        !bus.calls.lock().unwrap().is_empty()
    });
    notifier.show(&Message::Finishing, Urgency::Normal);
    wait_for("replacement", || bus.calls.lock().unwrap().len() >= 2);
    let calls = bus.calls.lock().unwrap();
    assert_eq!(calls[1].summary, "Finishing");
    assert_eq!(calls[1].replaces_id, 1);
    drop(calls);
    notifier.close();
    wait_for("close notification call", || {
        bus.closed.lock().unwrap().contains(&1)
    });
}

#[test]
fn one_time_notice_is_independent_of_the_main_notification() {
    let bus = PrivateNotifications::start();
    let (mut notifier, _) = notifier(&bus);
    notifier.show(&Message::Listening, Urgency::Normal);
    notifier.notice_once(OneTimeNotice::MynaUnavailable);
    wait_for("both notification calls", || {
        bus.calls.lock().unwrap().len() >= 2
    });
    let calls = bus.calls.lock().unwrap();
    assert_eq!(calls[0].summary, "Listening");
    assert_eq!(calls[1].summary, "Myna unavailable");
    assert_eq!(calls[1].replaces_id, 0);
}
