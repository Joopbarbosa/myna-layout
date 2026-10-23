//! Integration tests: the Myna client against the fake Myna on a private bus.
//!
//! None of these touch the user's session bus or the real Myna.

use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread::sleep,
    time::{Duration, Instant},
};

use myna_layout::{
    core::{Event, EventSink, MynaClient, MynaState},
    myna::DbusMynaClient,
};

use crate::fake_myna::FakeMyna;

type Collected = Arc<Mutex<Vec<Event>>>;

fn collector() -> (EventSink, Collected) {
    let events: Collected = Arc::default();
    let sink = {
        let events = Arc::clone(&events);
        Box::new(move |event: Event| events.lock().unwrap().push(event))
    };
    (sink, events)
}

/// Wait until `condition` holds for the events collected so far, or fail after 5 s.
fn wait_for(events: &Collected, what: &str, condition: impl Fn(&[Event]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if condition(&events.lock().unwrap()) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; got {:?}",
            events.lock().unwrap()
        );
        sleep(Duration::from_millis(20));
    }
}

fn recording_level(events: &[Event]) -> Option<f32> {
    events.iter().rev().find_map(|event| match event {
        Event::Myna(reading) if reading.state == MynaState::Recording => Some(reading.level),
        _ => None,
    })
}

/// A private bus with nothing on it.
struct EmptyBus {
    address: String,
    daemon: Child,
}

impl EmptyBus {
    fn start() -> Self {
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon must start for the Myna client integration tests");
        let mut line = String::new();
        BufReader::new(daemon.stdout.take().expect("bus output"))
            .read_line(&mut line)
            .expect("bus address");
        Self {
            address: line.trim().to_owned(),
            daemon,
        }
    }
}

impl Drop for EmptyBus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

#[test]
fn the_first_reading_comes_before_the_presence() {
    let fake = FakeMyna::start().expect("fake Myna must start");
    let (sink, events) = collector();
    let _client = DbusMynaClient::connect(Some(fake.bus_address()), sink).expect("connect");
    wait_for(&events, "presence", |events| {
        events.contains(&Event::MynaPresent)
    });
    let events = events.lock().unwrap();
    assert!(matches!(&events[0], Event::Myna(reading) if reading.state == MynaState::Idle));
    assert_eq!(events[1], Event::MynaPresent);
}

#[test]
fn states_and_level_follow_the_service() {
    let fake = FakeMyna::start().expect("fake Myna must start");
    let (sink, events) = collector();
    let _client = DbusMynaClient::connect(Some(fake.bus_address()), sink).expect("connect");
    wait_for(&events, "presence", |events| {
        events.contains(&Event::MynaPresent)
    });

    fake.set_state("recording", "Listening", 0.12).unwrap();
    wait_for(&events, "a recording reading", |events| {
        recording_level(events).is_some()
    });
    let level = recording_level(&events.lock().unwrap()).unwrap();
    assert!((level - 0.12).abs() < 1e-4, "level was {level}");

    fake.set_state("sleeping", "", 0.0).unwrap();
    wait_for(&events, "an unknown state", |events| {
        events.iter().any(|event| matches!(event, Event::Myna(r) if r.state == MynaState::Unknown("sleeping".into())))
    });

    fake.set_state("error", "No text field focused", 0.0)
        .unwrap();
    wait_for(&events, "an error reading with its reason", |events| {
        events.iter().any(|event| matches!(event, Event::Myna(r)
            if r.state == MynaState::Error && r.text.as_deref().is_some_and(|t| t.contains("No text field focused"))))
    });
}

#[test]
fn cen_novo_6_a_restarted_myna_is_noticed_and_registration_can_be_redone() {
    let fake = FakeMyna::start().expect("fake Myna must start");
    let (sink, events) = collector();
    let mut client = DbusMynaClient::connect(Some(fake.bus_address()), sink).expect("connect");
    wait_for(&events, "presence", |events| {
        events.contains(&Event::MynaPresent)
    });

    client.register();
    let deadline = Instant::now() + Duration::from_secs(5);
    while fake.registered_clients().unwrap() != 1 {
        assert!(Instant::now() < deadline, "the client never registered");
        sleep(Duration::from_millis(20));
    }

    events.lock().unwrap().clear();
    fake.restart().unwrap();
    wait_for(&events, "absence then presence again", |events| {
        let absent = events.iter().position(|event| *event == Event::MynaAbsent);
        absent.is_some_and(|at| events[at..].contains(&Event::MynaPresent))
    });
    // A restarted Myna starts with an empty registry: the core must ask again.
    assert_eq!(fake.registered_clients().unwrap(), 0);
    client.register();
    let deadline = Instant::now() + Duration::from_secs(5);
    while fake.registered_clients().unwrap() != 1 {
        assert!(
            Instant::now() < deadline,
            "the client could not register again"
        );
        sleep(Duration::from_millis(20));
    }
    // Registering is the only call the client makes on Myna (RN-NOVA-6).
    assert_eq!(fake.toggle_calls().unwrap(), 0);
}

#[test]
fn cen_novo_5_an_absent_myna_is_reported_once_and_nothing_else() {
    let bus = EmptyBus::start();
    let (sink, events) = collector();
    let _client = DbusMynaClient::connect(Some(&bus.address), sink).expect("connect");
    wait_for(&events, "absence", |events| {
        events.contains(&Event::MynaAbsent)
    });
    sleep(Duration::from_millis(300));
    assert_eq!(*events.lock().unwrap(), vec![Event::MynaAbsent]);
}

#[test]
fn cen_novo_5_myna_appearing_later_is_picked_up() {
    let bus = EmptyBus::start();
    let (sink, events) = collector();
    let _client = DbusMynaClient::connect(Some(&bus.address), sink).expect("connect");
    wait_for(&events, "absence", |events| {
        events.contains(&Event::MynaAbsent)
    });
    let service = Command::new(env!("CARGO_BIN_EXE_fake-myna"))
        .env("FAKE_MYNA_BUS_ADDRESS", &bus.address)
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("fake Myna binary");
    let mut service = ServiceGuard(service);
    wait_for(&events, "presence after the late start", |events| {
        events.contains(&Event::MynaPresent)
    });
    assert!(service.0.try_wait().unwrap().is_none());
}

struct ServiceGuard(Child);

impl Drop for ServiceGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// An object that owns Myna's name but lacks the properties the indicator needs.
struct OddMyna;

#[zbus::interface(name = "com.canonical.Myna.Dictation")]
impl OddMyna {
    fn toggle(&self) {}
}

#[test]
fn cen_novo_7_an_interface_without_the_expected_properties_is_incompatible() {
    let bus = EmptyBus::start();
    let _service = zbus::block_on(
        zbus::connection::Builder::address(bus.address.as_str())
            .name("com.canonical.Myna.Dictation")
            .serve_at("/com/canonical/Myna/Dictation", OddMyna)
            .build(),
    )
    .expect("odd Myna service");
    let (sink, events) = collector();
    let _client = DbusMynaClient::connect(Some(&bus.address), sink).expect("connect");
    wait_for(&events, "incompatibility", |events| {
        events.contains(&Event::MynaIncompatible)
    });
    assert!(!events.lock().unwrap().contains(&Event::MynaPresent));
}
