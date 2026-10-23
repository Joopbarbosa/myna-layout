//! Integration tests: the whole program against the fake Myna and a mock notification
//! service, on a private bus and with no Wayland display (so the notification is the
//! surface). It checks the wiring end to end: registration, states, restart, shutdown.

use std::{
    collections::HashMap,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread::sleep,
    time::{Duration, Instant},
};

use zbus::OwnedVariant;

use crate::fake_myna::FakeMyna;

#[derive(Default)]
struct Record {
    shown: Vec<(u32, String, u8)>,
    closed: Vec<u32>,
}

struct MockNotifications {
    record: Arc<Mutex<Record>>,
    next_id: Mutex<u32>,
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
            .and_then(|v| u8::try_from(v).ok())
            .unwrap_or(0);
        let mut next = self.next_id.lock().unwrap();
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            *next += 1;
            *next
        };
        self.record
            .lock()
            .unwrap()
            .shown
            .push((id, summary, urgency));
        id
    }

    fn close_notification(&self, id: u32) {
        self.record.lock().unwrap().closed.push(id);
    }
}

/// The program under test, killed when the test ends.
struct Program(Child);

impl Drop for Program {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        sleep(Duration::from_millis(25));
    }
}

fn start(fake: &FakeMyna) -> (Program, Arc<Mutex<Record>>, zbus::Connection) {
    let record = Arc::new(Mutex::new(Record::default()));
    let service = zbus::block_on(
        zbus::connection::Builder::address(fake.bus_address())
            .name("org.freedesktop.Notifications")
            .serve_at(
                "/org/freedesktop/Notifications",
                MockNotifications {
                    record: Arc::clone(&record),
                    next_id: Mutex::new(0),
                },
            )
            .build(),
    )
    .expect("mock notification service");
    let config_home = std::env::temp_dir().join(format!("myna-layout-test-{}", std::process::id()));
    let child = Command::new(env!("CARGO_BIN_EXE_myna-layout"))
        .env("MYNA_LAYOUT_BUS_ADDRESS", fake.bus_address())
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .env_remove("WAYLAND_DISPLAY")
        .env("LC_ALL", "en_US.UTF-8")
        .env("XDG_CONFIG_HOME", config_home)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the program must start");
    (Program(child), record, service)
}

#[test]
fn the_program_registers_shows_the_states_and_closes_at_idle() {
    let fake = FakeMyna::start().expect("fake Myna must start");
    let (_program, record, _service) = start(&fake);
    wait_until("the registration", || {
        fake.registered_clients().unwrap() == 1
    });

    fake.set_state("loading", "", 0.0).unwrap();
    wait_until("the loading notification", || {
        record
            .lock()
            .unwrap()
            .shown
            .iter()
            .any(|(_, text, _)| text == "Loading")
    });
    fake.set_state("recording", "", 0.1).unwrap();
    wait_until("the listening notification", || {
        record
            .lock()
            .unwrap()
            .shown
            .iter()
            .any(|(_, text, _)| text == "Listening")
    });
    fake.set_state("idle", "", 0.0).unwrap();
    wait_until("the close at idle", || {
        !record.lock().unwrap().closed.is_empty()
    });

    let record = record.lock().unwrap();
    let ids: std::collections::HashSet<_> = record.shown.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(
        ids.len(),
        1,
        "all states must replace the same notification: {:?}",
        record.shown
    );
    assert!(
        record.shown.iter().all(|(_, _, urgency)| *urgency == 1),
        "never low, never critical here"
    );
    assert_eq!(
        fake.toggle_calls().unwrap(),
        0,
        "the indicator never toggles dictation"
    );
}

#[test]
fn cen_novo_6_a_restarted_myna_gets_the_registration_again() {
    let fake = FakeMyna::start().expect("fake Myna must start");
    let (_program, _record, _service) = start(&fake);
    wait_until("the first registration", || {
        fake.registered_clients().unwrap() == 1
    });
    fake.restart().unwrap();
    // The restarted Myna starts empty and the indicator registers again by itself.
    wait_until("the second registration", || {
        fake.registered_clients().unwrap() == 1
    });
}

#[test]
fn shutdown_unregisters_before_the_process_exits() {
    let fake = FakeMyna::start().expect("fake Myna must start");
    let (mut program, _record, _service) = start(&fake);
    wait_until("the registration", || {
        fake.registered_clients().unwrap() == 1
    });
    Command::new("kill")
        .args(["-TERM", &program.0.id().to_string()])
        .status()
        .expect("kill");
    wait_until("the process to exit", || {
        program.0.try_wait().unwrap().is_some()
    });
    assert_eq!(
        fake.registered_clients().unwrap(),
        0,
        "the unregistration must reach Myna"
    );
}
