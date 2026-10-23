//! Runs the commands of the `core` against the real modules.
//!
//! [`step`] applies one event to the state and carries out the commands it returns.
//! A module that could not be created is represented by `None`: asking it to start
//! reports a failure on the spot (RN-NOVA-10, RN-NOVA-18), and every other command
//! for it is dropped.

use std::time::Duration;

use crate::core::{Bar, Command, Event, MynaClient, Notifier, State, Surface, decide};

/// The modules the commands are carried out on.
#[derive(Default)]
pub struct Ports<'a> {
    /// The floating bar, if it could connect to the compositor.
    pub bar: Option<&'a mut dyn Bar>,
    /// The notification surface, if it could connect to the bus.
    pub notifier: Option<&'a mut dyn Notifier>,
    /// The Myna client.
    pub client: Option<&'a mut dyn MynaClient>,
}

/// Apply one event. Returns the new state and the events the runtime produced itself
/// (a surface that does not exist answers "failed" right away); the caller feeds them
/// back in.
pub fn step(state: State, event: Event, ports: &mut Ports<'_>) -> (State, Vec<Event>) {
    let (state, commands) = decide(state, event);
    let follow_ups = commands
        .into_iter()
        .filter_map(|command| execute(command, ports))
        .collect();
    (state, follow_ups)
}

/// Apply an event and everything it causes, until nothing is left.
pub fn run_all(state: State, event: Event, ports: &mut Ports<'_>) -> State {
    let mut state = state;
    let mut queue = std::collections::VecDeque::from([event]);
    while let Some(event) = queue.pop_front() {
        let (next, follow_ups) = step(state, event, ports);
        state = next;
        queue.extend(follow_ups);
    }
    state
}

/// Wait for the notification and Myna workers to finish what they were asked, so the
/// last close and the unregistration are not lost when the process ends.
pub fn flush(ports: &mut Ports<'_>, timeout: Duration) {
    if let Some(notifier) = ports.notifier.as_deref_mut() {
        notifier.flush(timeout);
    }
    if let Some(client) = ports.client.as_deref_mut() {
        client.flush(timeout);
    }
}

fn execute(command: Command, ports: &mut Ports<'_>) -> Option<Event> {
    match command {
        Command::StartSurface(Surface::Bar) => match ports.bar.as_deref_mut() {
            Some(bar) => bar.start(),
            None => return Some(Event::SurfaceFailed(Surface::Bar)),
        },
        Command::StartSurface(Surface::Notification) => match ports.notifier.as_deref_mut() {
            Some(notifier) => notifier.start(),
            None => return Some(Event::SurfaceFailed(Surface::Notification)),
        },
        Command::ShowBar(look) => {
            if let Some(bar) = ports.bar.as_deref_mut() {
                bar.show(&look);
            }
        }
        Command::UpdateLevel(level) => {
            if let Some(bar) = ports.bar.as_deref_mut() {
                bar.update_level(level);
            }
        }
        Command::HideBar => {
            if let Some(bar) = ports.bar.as_deref_mut() {
                bar.hide();
            }
        }
        Command::ShowNotification { message, urgency } => {
            if let Some(notifier) = ports.notifier.as_deref_mut() {
                notifier.show(&message, urgency);
            }
        }
        Command::CloseNotification => {
            if let Some(notifier) = ports.notifier.as_deref_mut() {
                notifier.close();
            }
        }
        Command::NoticeOnce(notice) => {
            if let Some(notifier) = ports.notifier.as_deref_mut() {
                notifier.notice_once(notice);
            } else {
                tracing::warn!(?notice, "no notification surface to show the warning");
            }
        }
        Command::Register => {
            if let Some(client) = ports.client.as_deref_mut() {
                client.register();
            }
        }
        Command::Unregister => {
            if let Some(client) = ports.client.as_deref_mut() {
                client.unregister();
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::core::{
        BarKind, BarLook, Config, Message, MynaReading, MynaState, OneTimeNotice, Urgency,
    };

    type Log = Arc<Mutex<Vec<String>>>;

    struct FakeBar(Log);
    struct FakeNotifier(Log);
    struct FakeClient(Log);

    fn note(log: &Log, text: impl Into<String>) {
        log.lock().unwrap().push(text.into());
    }

    impl Bar for FakeBar {
        fn start(&mut self) {
            note(&self.0, "bar.start");
        }
        fn show(&mut self, look: &BarLook) {
            note(&self.0, format!("bar.show {:?}", look.kind));
        }
        fn update_level(&mut self, level: f32) {
            note(&self.0, format!("bar.level {level:.2}"));
        }
        fn hide(&mut self) {
            note(&self.0, "bar.hide");
        }
    }

    impl Notifier for FakeNotifier {
        fn start(&mut self) {
            note(&self.0, "notifier.start");
        }
        fn show(&mut self, message: &Message, urgency: Urgency) {
            note(&self.0, format!("notifier.show {message:?} {urgency:?}"));
        }
        fn notice_once(&mut self, notice: OneTimeNotice) {
            note(&self.0, format!("notifier.once {notice:?}"));
        }
        fn close(&mut self) {
            note(&self.0, "notifier.close");
        }
        fn flush(&mut self, _: Duration) {
            note(&self.0, "notifier.flush");
        }
    }

    impl MynaClient for FakeClient {
        fn register(&mut self) {
            note(&self.0, "client.register");
        }
        fn unregister(&mut self) {
            note(&self.0, "client.unregister");
        }
        fn version(&self) -> Option<String> {
            None
        }
        fn flush(&mut self, _: Duration) {
            note(&self.0, "client.flush");
        }
    }

    fn reading(state: MynaState, level: f32) -> Event {
        Event::Myna(MynaReading {
            state,
            text: None,
            level,
        })
    }

    #[test]
    fn a_dictation_runs_from_the_first_surface_to_the_unregistration() {
        let log = Log::default();
        let (mut bar, mut notifier, mut client) = (
            FakeBar(log.clone()),
            FakeNotifier(log.clone()),
            FakeClient(log.clone()),
        );
        let mut ports = Ports {
            bar: Some(&mut bar),
            notifier: Some(&mut notifier),
            client: Some(&mut client),
        };
        let mut state = State::new(Config::default());
        for event in [
            Event::ConfigLoaded(Config::default()),
            Event::SurfaceReady(Surface::Bar),
            Event::SurfaceReady(Surface::Notification),
            Event::MynaPresent,
            reading(MynaState::Loading, 0.0),
            reading(MynaState::Recording, 0.2),
            reading(MynaState::Idle, 0.0),
            Event::Shutdown,
        ] {
            state = run_all(state, event, &mut ports);
        }
        flush(&mut ports, Duration::from_millis(1));
        assert_eq!(
            *log.lock().unwrap(),
            vec![
                "bar.start",
                "notifier.start",
                "client.register",
                "bar.show Loading",
                "bar.show Recording",
                "bar.level 0.20",
                "bar.hide",
                "client.unregister",
                "notifier.flush",
                "client.flush",
            ]
        );
    }

    #[test]
    fn cen_novo_8_a_missing_bar_fails_at_once_and_the_notification_takes_over() {
        let log = Log::default();
        let (mut notifier, mut client) = (FakeNotifier(log.clone()), FakeClient(log.clone()));
        let mut ports = Ports {
            bar: None,
            notifier: Some(&mut notifier),
            client: Some(&mut client),
        };
        let mut state = State::new(Config::default());
        for event in [
            Event::ConfigLoaded(Config::default()),
            Event::SurfaceReady(Surface::Notification),
            Event::MynaPresent,
            reading(MynaState::Loading, 0.0),
        ] {
            state = run_all(state, event, &mut ports);
        }
        let log = log.lock().unwrap();
        assert!(log.contains(&"client.register".to_owned()), "{log:?}");
        assert!(
            log.contains(&"notifier.show Loading Normal".to_owned()),
            "{log:?}"
        );
    }

    #[test]
    fn with_no_surface_at_all_the_indicator_never_registers() {
        let log = Log::default();
        let mut client = FakeClient(log.clone());
        let mut ports = Ports {
            bar: None,
            notifier: None,
            client: Some(&mut client),
        };
        let mut state = State::new(Config::default());
        for event in [Event::ConfigLoaded(Config::default()), Event::MynaPresent] {
            state = run_all(state, event, &mut ports);
        }
        let _ = state;
        assert!(log.lock().unwrap().is_empty());
    }

    #[test]
    fn bar_kinds_reach_the_bar_unchanged() {
        let log = Log::default();
        let mut bar = FakeBar(log.clone());
        let mut ports = Ports {
            bar: Some(&mut bar),
            ..Ports::default()
        };
        let look = BarLook {
            kind: BarKind::Notice,
            message: None,
        };
        assert_eq!(execute(Command::ShowBar(look), &mut ports), None);
        assert_eq!(*log.lock().unwrap(), vec!["bar.show Notice"]);
    }
}
