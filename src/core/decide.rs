//! The state function: `decide(state, event) -> (state, commands)`.
//!
//! This is where the rules of the indicator live (RN-NOVA-1 to 7, 10, 11, 13, 18
//! and 19). It is pure: no D-Bus, no Wayland, no clock. Every outside effect is a
//! [`Command`] and every outside fact is an [`Event`]. [`Command`] has no way to
//! start, stop or toggle dictation or to type, so the indicator cannot control Myna
//! (RN-NOVA-6); the only commands that reach it are register and unregister.
//!
//! Reading guide:
//!
//! 1. [`decide`] applies one event to the facts the state keeps (presence of Myna,
//!    last reading, status of each surface).
//! 2. Registration is reconciled: the indicator is registered in Myna exactly while
//!    Myna is present and at least one surface works (RN-NOVA-5).
//! 3. What must be on screen is recomputed from the facts and compared with what is
//!    already shown, so only the differences become commands.

use super::{
    BarKind, BarLook, Command, Config, Event, Message, Mode, MynaReading, MynaState, OneTimeNotice,
    Surface, Urgency,
};

/// Whether Myna can be reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Presence {
    /// Nothing known yet.
    Unknown,
    /// Myna is on the bus and speaks the expected interface.
    Present,
    /// Myna is not on the bus.
    Absent,
    /// Myna is on the bus but the interface is not the expected one.
    Incompatible,
}

/// The life cycle of one surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SurfaceStatus {
    /// Not requested (yet): the mode does not use it, or no start was asked.
    NotStarted,
    /// A start was asked and the answer has not arrived.
    Starting,
    /// It answered ready and has not failed since.
    Ready,
    /// It could not start or stopped working.
    Failed,
}

/// The state of the indicator.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    config: Config,
    presence: Presence,
    reading: MynaReading,
    bar: SurfaceStatus,
    notification: SurfaceStatus,
    registered: bool,
    /// The one-time warning was already shown in the current period of unavailability.
    one_time_notice_shown: bool,
    /// The notice Myna was already showing through its own toast when the indicator
    /// registered must not be shown again (RN-NOVA-11 and RN-NOVA-19).
    suppress_current_notice: bool,
    shown_bar: Option<BarLook>,
    shown_level: f32,
    shown_notification: Option<(Message, Urgency)>,
    shutting_down: bool,
}

impl State {
    /// The state at start-up, with the given configuration.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            presence: Presence::Unknown,
            reading: MynaReading {
                state: MynaState::Idle,
                text: None,
                level: 0.0,
            },
            bar: SurfaceStatus::NotStarted,
            notification: SurfaceStatus::NotStarted,
            registered: false,
            one_time_notice_shown: false,
            suppress_current_notice: false,
            shown_bar: None,
            shown_level: 0.0,
            shown_notification: None,
            shutting_down: false,
        }
    }

    /// The audio level the indicator uses: Myna's level only while recording,
    /// clamped to 0 to 1, and 0 in every other state (RN-NOVA-2).
    pub fn effective_level(&self) -> f32 {
        match self.reading.state {
            MynaState::Recording if self.reading.level.is_finite() => {
                self.reading.level.clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }

    fn requests_bar(&self) -> bool {
        self.config.mode != Mode::NotificationOnly
    }

    fn requests_notification(&self) -> bool {
        self.config.mode != Mode::BarOnly
    }

    fn bar_works(&self) -> bool {
        self.bar == SurfaceStatus::Ready
    }

    fn notification_works(&self) -> bool {
        self.notification == SurfaceStatus::Ready
    }

    fn some_surface_works(&self) -> bool {
        self.bar_works() || self.notification_works()
    }

    /// Myna is gone or incompatible: nothing is shown, the indicator unregisters and
    /// the one-time warning is shown (RN-NOVA-3, RN-NOVA-5, RN-NOVA-7).
    fn become_unavailable(
        &mut self,
        presence: Presence,
        notice: OneTimeNotice,
        out: &mut Vec<Command>,
    ) {
        self.presence = presence;
        self.reading = MynaReading {
            state: MynaState::Idle,
            text: None,
            level: 0.0,
        };
        self.suppress_current_notice = false;
        self.hide_everything(out);
        if self.registered {
            self.registered = false;
            out.push(Command::Unregister);
        }
        if !self.one_time_notice_shown {
            self.one_time_notice_shown = true;
            out.push(Command::NoticeOnce(notice));
        }
    }

    fn hide_everything(&mut self, out: &mut Vec<Command>) {
        if self.shown_bar.take().is_some() {
            self.shown_level = 0.0;
            out.push(Command::HideBar);
        }
        if self.shown_notification.take().is_some() {
            out.push(Command::CloseNotification);
        }
    }

    /// Register while Myna is present and some surface works; unregister otherwise
    /// (RN-NOVA-5).
    fn reconcile_registration(&mut self, out: &mut Vec<Command>) {
        let wanted = self.presence == Presence::Present && self.some_surface_works();
        if wanted && !self.registered {
            self.registered = true;
            // Myna already shows a notice through its own toast: do not show it twice.
            self.suppress_current_notice = self.reading.state == MynaState::Notice;
            out.push(Command::Register);
        } else if !wanted && self.registered {
            self.registered = false;
            out.push(Command::Unregister);
        }
    }

    fn bar_look(&self) -> Option<BarLook> {
        let text = || self.reading.text.clone().unwrap_or_default();
        let (kind, message) = match &self.reading.state {
            MynaState::Idle => return None,
            MynaState::Notice if self.suppress_current_notice => return None,
            MynaState::Loading => (BarKind::Loading, Some(Message::Loading)),
            MynaState::Recording => (BarKind::Recording, Some(Message::Listening)),
            MynaState::Transcribing | MynaState::Finalizing => {
                (BarKind::Finishing, Some(Message::Finishing))
            }
            MynaState::Notice => (BarKind::Notice, Some(Message::Notice(text()))),
            MynaState::Error => (BarKind::Error, Some(Message::Error(text()))),
            MynaState::Unknown(_) => (BarKind::Neutral, None),
        };
        Some(BarLook { kind, message })
    }

    /// The notification to show now, if any (RN-NOVA-8, RN-NOVA-11, RN-NOVA-19).
    fn notification_view(&self) -> Option<(Message, Urgency)> {
        if !self.notification_works() {
            return None;
        }
        let text = || self.reading.text.clone().unwrap_or_default();
        if self.bar_works() {
            // The bar shows the states; the notification only adds the reason of an
            // error, and only in the mode that asks for both.
            return (self.config.mode == Mode::Both && self.reading.state == MynaState::Error)
                .then(|| (Message::Error(text()), Urgency::Critical));
        }
        // The notification is the surface.
        match &self.reading.state {
            MynaState::Idle | MynaState::Unknown(_) => None,
            MynaState::Notice if self.suppress_current_notice => None,
            MynaState::Loading => Some((Message::Loading, Urgency::Normal)),
            MynaState::Recording => Some((Message::Listening, Urgency::Normal)),
            MynaState::Transcribing | MynaState::Finalizing => {
                Some((Message::Finishing, Urgency::Normal))
            }
            MynaState::Notice => Some((Message::Notice(text()), Urgency::Normal)),
            MynaState::Error => Some((Message::Error(text()), Urgency::Critical)),
        }
    }

    /// Compare what must be on screen with what is shown and emit the differences.
    fn render(&mut self, out: &mut Vec<Command>) {
        if self.presence != Presence::Present {
            return;
        }
        let bar_look = if self.bar_works() {
            self.bar_look()
        } else {
            None
        };
        match (&self.shown_bar, bar_look) {
            (None, None) => {}
            (Some(_), None) => {
                self.shown_bar = None;
                self.shown_level = 0.0;
                out.push(Command::HideBar);
            }
            (shown, Some(look)) => {
                if shown.as_ref() != Some(&look) {
                    self.shown_level = 0.0;
                    out.push(Command::ShowBar(look.clone()));
                }
                let level = self.effective_level();
                if look.kind == BarKind::Recording
                    && (level - self.shown_level).abs() > f32::EPSILON
                {
                    self.shown_level = level;
                    out.push(Command::UpdateLevel(level));
                }
                self.shown_bar = Some(look);
            }
        }

        let notification = self.notification_view();
        match (&self.shown_notification, notification) {
            (None, None) => {}
            (Some(_), None) => {
                self.shown_notification = None;
                out.push(Command::CloseNotification);
            }
            (shown, Some(view)) => {
                if shown.as_ref() != Some(&view) {
                    out.push(Command::ShowNotification {
                        message: view.0.clone(),
                        urgency: view.1,
                    });
                }
                self.shown_notification = Some(view);
            }
        }
    }

    /// Ask for the surfaces the mode uses (DT-NOVA-3).
    fn start_surfaces(&mut self, out: &mut Vec<Command>) {
        self.bar = SurfaceStatus::NotStarted;
        self.notification = SurfaceStatus::NotStarted;
        if self.requests_bar() {
            self.bar = SurfaceStatus::Starting;
            out.push(Command::StartSurface(Surface::Bar));
        }
        if self.requests_notification() {
            self.notification = SurfaceStatus::Starting;
            out.push(Command::StartSurface(Surface::Notification));
        }
    }

    fn status_mut(&mut self, surface: Surface) -> &mut SurfaceStatus {
        match surface {
            Surface::Bar => &mut self.bar,
            Surface::Notification => &mut self.notification,
        }
    }

    /// A surface stopped working: forget what it showed, and if the bar was the one,
    /// bring the notification up as the surface (RN-NOVA-10, RN-NOVA-18).
    fn surface_failed(&mut self, surface: Surface, out: &mut Vec<Command>) {
        *self.status_mut(surface) = SurfaceStatus::Failed;
        match surface {
            Surface::Bar => {
                self.shown_bar = None;
                self.shown_level = 0.0;
                if self.notification == SurfaceStatus::NotStarted {
                    self.notification = SurfaceStatus::Starting;
                    out.push(Command::StartSurface(Surface::Notification));
                }
            }
            Surface::Notification => self.shown_notification = None,
        }
    }
}

/// Apply an event to the state and return the new state and the commands to run.
pub fn decide(mut state: State, event: Event) -> (State, Vec<Command>) {
    let mut out = Vec::new();
    if state.shutting_down {
        return (state, out);
    }
    match event {
        Event::ConfigLoaded(config) => {
            state.config = config;
            state.start_surfaces(&mut out);
        }
        Event::MynaPresent => {
            state.presence = Presence::Present;
            state.one_time_notice_shown = false;
        }
        Event::MynaAbsent => {
            state.become_unavailable(Presence::Absent, OneTimeNotice::MynaUnavailable, &mut out);
        }
        Event::MynaIncompatible => {
            state.become_unavailable(
                Presence::Incompatible,
                OneTimeNotice::MynaIncompatible,
                &mut out,
            );
        }
        Event::Myna(reading) => {
            if state.presence != Presence::Present {
                state.presence = Presence::Present;
                state.one_time_notice_shown = false;
            }
            if let MynaState::Unknown(name) = &reading.state
                && state.reading.state != reading.state
            {
                tracing::warn!(state = %name, "Myna published a state this version does not know");
            }
            if reading.state != MynaState::Notice {
                state.suppress_current_notice = false;
            }
            state.reading = reading;
        }
        Event::SurfaceReady(surface) => *state.status_mut(surface) = SurfaceStatus::Ready,
        Event::SurfaceFailed(surface) => state.surface_failed(surface, &mut out),
        Event::Shutdown => {
            state.shutting_down = true;
            state.hide_everything(&mut out);
            if state.registered {
                state.registered = false;
                out.push(Command::Unregister);
            }
            return (state, out);
        }
    }
    state.reconcile_registration(&mut out);
    state.render(&mut out);
    (state, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Config, Mode};

    fn reading(state: MynaState, text: Option<&str>, level: f32) -> Event {
        Event::Myna(MynaReading {
            state,
            text: text.map(str::to_owned),
            level,
        })
    }

    /// Feed events one by one and return the final state plus the commands of each step.
    fn run(config: Config, events: Vec<Event>) -> (State, Vec<Vec<Command>>) {
        let mut state = State::new(config);
        let mut steps = Vec::new();
        for event in events {
            let (next, commands) = decide(state, event);
            state = next;
            steps.push(commands);
        }
        (state, steps)
    }

    fn mode(mode: Mode) -> Config {
        Config {
            mode,
            ..Config::default()
        }
    }

    fn all(steps: &[Vec<Command>]) -> Vec<Command> {
        steps.iter().flatten().cloned().collect()
    }

    fn started(mode_: Mode) -> Vec<Event> {
        let mut events = vec![Event::ConfigLoaded(mode(mode_)), Event::MynaPresent];
        if mode_ != Mode::NotificationOnly {
            events.push(Event::SurfaceReady(Surface::Bar));
        }
        if mode_ != Mode::BarOnly {
            events.push(Event::SurfaceReady(Surface::Notification));
        }
        events
    }

    fn look(kind: BarKind, message: Option<Message>) -> Command {
        Command::ShowBar(BarLook { kind, message })
    }

    #[test]
    fn cen_novo_1_full_cycle_shows_loading_listening_finishing_then_hides() {
        let mut events = started(Mode::Both);
        events.extend([
            reading(MynaState::Loading, None, 0.0),
            reading(MynaState::Recording, None, 0.1),
            reading(MynaState::Transcribing, None, 0.0),
            reading(MynaState::Finalizing, None, 0.0),
            reading(MynaState::Idle, None, 0.0),
        ]);
        let (_, steps) = run(Config::default(), events);
        let commands = all(&steps);
        let bar: Vec<_> = commands
            .iter()
            .filter(|c| matches!(c, Command::ShowBar(_) | Command::HideBar))
            .cloned()
            .collect();
        assert_eq!(
            bar,
            vec![
                look(BarKind::Loading, Some(Message::Loading)),
                look(BarKind::Recording, Some(Message::Listening)),
                look(BarKind::Finishing, Some(Message::Finishing)),
                Command::HideBar,
            ]
        );
        assert!(commands.contains(&Command::Register));
    }

    #[test]
    fn cen_novo_2_level_is_zero_outside_recording() {
        let mut state = State::new(Config::default());
        state.reading = MynaReading {
            state: MynaState::Loading,
            text: None,
            level: 1.0,
        };
        assert_eq!(state.effective_level(), 0.0);
        let mut events = started(Mode::Both);
        events.push(reading(MynaState::Loading, None, 1.0));
        let (_, steps) = run(Config::default(), events);
        assert!(
            !all(&steps)
                .iter()
                .any(|c| matches!(c, Command::UpdateLevel(_)))
        );
    }

    #[test]
    fn cen_novo_3_level_above_one_is_clamped() {
        let mut state = State::new(Config::default());
        state.reading = MynaReading {
            state: MynaState::Recording,
            text: None,
            level: 1.7,
        };
        assert_eq!(state.effective_level(), 1.0);
        state.reading.level = -0.5;
        assert_eq!(state.effective_level(), 0.0);
        state.reading.level = f32::NAN;
        assert_eq!(state.effective_level(), 0.0);
        state.reading.level = f32::INFINITY;
        assert_eq!(state.effective_level(), 0.0);
    }

    #[test]
    fn cen_novo_4_unknown_state_is_neutral_and_does_not_stop_the_indicator() {
        let mut events = started(Mode::Both);
        events.push(reading(MynaState::Unknown("sleeping".into()), None, 0.0));
        events.push(reading(MynaState::Recording, None, 0.05));
        let (_, steps) = run(Config::default(), events);
        let commands = all(&steps);
        assert!(commands.contains(&look(BarKind::Neutral, None)));
        assert!(commands.contains(&look(BarKind::Recording, Some(Message::Listening))));
    }

    #[test]
    fn cen_novo_5_absent_myna_warns_once_and_draws_nothing() {
        let events = vec![
            Event::ConfigLoaded(Config::default()),
            Event::SurfaceReady(Surface::Bar),
            Event::SurfaceReady(Surface::Notification),
            Event::MynaAbsent,
            Event::MynaAbsent,
            Event::MynaAbsent,
        ];
        let (_, steps) = run(Config::default(), events);
        let commands = all(&steps);
        let notices = commands
            .iter()
            .filter(|c| matches!(c, Command::NoticeOnce(_)))
            .count();
        assert_eq!(notices, 1);
        assert!(commands.contains(&Command::NoticeOnce(OneTimeNotice::MynaUnavailable)));
        assert!(!commands.iter().any(|c| matches!(c, Command::ShowBar(_))));
        assert!(!commands.contains(&Command::Register));
    }

    #[test]
    fn cen_novo_5_the_warning_comes_back_after_myna_returns_and_leaves_again() {
        let events = vec![Event::MynaAbsent, Event::MynaPresent, Event::MynaAbsent];
        let (_, steps) = run(Config::default(), events);
        let notices = all(&steps)
            .iter()
            .filter(|c| matches!(c, Command::NoticeOnce(_)))
            .count();
        assert_eq!(notices, 2);
    }

    #[test]
    fn cen_novo_6_restarted_myna_is_registered_again() {
        let mut events = started(Mode::Both);
        events.extend([Event::MynaAbsent, Event::MynaPresent]);
        let (_, steps) = run(Config::default(), events);
        let commands = all(&steps);
        assert_eq!(
            commands.iter().filter(|c| **c == Command::Register).count(),
            2
        );
        assert!(commands.contains(&Command::Unregister));
    }

    #[test]
    fn cen_novo_7_incompatible_myna_warns_once_and_unregisters() {
        let mut events = started(Mode::Both);
        events.extend([Event::MynaIncompatible, Event::MynaIncompatible]);
        let (_, steps) = run(Config::default(), events);
        let commands = all(&steps);
        let notices: Vec<_> = commands
            .iter()
            .filter(|c| matches!(c, Command::NoticeOnce(_)))
            .collect();
        assert_eq!(
            notices,
            vec![&Command::NoticeOnce(OneTimeNotice::MynaIncompatible)]
        );
        assert!(commands.contains(&Command::Unregister));
    }

    #[test]
    fn cen_novo_8_registers_with_the_notification_when_the_bar_cannot_be_created() {
        let events = vec![
            Event::ConfigLoaded(Config::default()),
            Event::MynaPresent,
            Event::SurfaceFailed(Surface::Bar),
            Event::SurfaceReady(Surface::Notification),
        ];
        let (state, steps) = run(Config::default(), events);
        assert!(all(&steps).contains(&Command::Register));
        assert!(state.registered);
    }

    #[test]
    fn cen_novo_12_bar_failing_mid_recording_moves_the_state_to_the_notification() {
        let mut events = started(Mode::Both);
        events.push(reading(MynaState::Recording, None, 0.1));
        events.push(Event::SurfaceFailed(Surface::Bar));
        events.push(reading(MynaState::Finalizing, None, 0.0));
        events.push(reading(MynaState::Idle, None, 0.0));
        let (state, steps) = run(Config::default(), events);
        let failure = &steps[steps.len() - 3];
        assert_eq!(
            failure,
            &vec![Command::ShowNotification {
                message: Message::Listening,
                urgency: Urgency::Normal
            }]
        );
        assert_eq!(
            steps[steps.len() - 2],
            vec![Command::ShowNotification {
                message: Message::Finishing,
                urgency: Urgency::Normal
            }]
        );
        assert_eq!(steps[steps.len() - 1], vec![Command::CloseNotification]);
        assert!(
            state.registered,
            "the indicator stays registered: the notification is a surface"
        );
        assert!(!all(&steps).contains(&Command::Unregister));
    }

    #[test]
    fn cen_novo_13_notification_only_mode_registers_and_notifies_every_state() {
        let mut events = started(Mode::NotificationOnly);
        events.extend([
            reading(MynaState::Loading, None, 0.0),
            reading(MynaState::Recording, None, 0.1),
            reading(MynaState::Finalizing, None, 0.0),
            reading(MynaState::Idle, None, 0.0),
        ]);
        let (state, steps) = run(mode(Mode::NotificationOnly), events);
        let commands = all(&steps);
        assert!(!commands.contains(&Command::StartSurface(Surface::Bar)));
        assert!(!commands.iter().any(|c| matches!(c, Command::ShowBar(_))));
        let notifications: Vec<_> = commands
            .iter()
            .filter_map(|c| match c {
                Command::ShowNotification { message, .. } => Some(message.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            notifications,
            vec![Message::Loading, Message::Listening, Message::Finishing]
        );
        assert!(commands.ends_with(&[Command::CloseNotification]));
        assert!(state.registered);
    }

    #[test]
    fn cen_novo_19_environment_without_a_bar_uses_the_notification() {
        let mut events = vec![
            Event::ConfigLoaded(Config::default()),
            Event::MynaPresent,
            Event::SurfaceFailed(Surface::Bar),
            Event::SurfaceReady(Surface::Notification),
        ];
        events.push(reading(MynaState::Loading, None, 0.0));
        let (state, steps) = run(Config::default(), events);
        let commands = all(&steps);
        assert!(commands.contains(&Command::ShowNotification {
            message: Message::Loading,
            urgency: Urgency::Normal
        }));
        assert!(state.registered);
    }

    #[test]
    fn cen_novo_26_notice_after_the_bar_failed_goes_to_the_notification() {
        let mut events = started(Mode::Both);
        events.push(reading(MynaState::Recording, None, 0.1));
        events.push(Event::SurfaceFailed(Surface::Bar));
        events.push(reading(MynaState::Notice, Some("No speech detected"), 0.0));
        let (_, steps) = run(Config::default(), events);
        assert_eq!(
            steps.last().unwrap(),
            &vec![Command::ShowNotification {
                message: Message::Notice("No speech detected".into()),
                urgency: Urgency::Normal
            }]
        );
    }

    #[test]
    fn cen_novo_27_no_working_surface_unregisters() {
        let mut events = started(Mode::Both);
        events.push(Event::SurfaceFailed(Surface::Bar));
        events.push(Event::SurfaceFailed(Surface::Notification));
        let (state, steps) = run(Config::default(), events);
        assert!(all(&steps).contains(&Command::Unregister));
        assert!(!state.registered);
    }

    #[test]
    fn cen_novo_28_a_notice_already_shown_by_myna_is_not_shown_again() {
        let events = vec![
            Event::ConfigLoaded(mode(Mode::NotificationOnly)),
            Event::MynaPresent,
            reading(MynaState::Notice, Some("No speech detected"), 0.0),
            Event::SurfaceReady(Surface::Notification),
            reading(MynaState::Loading, None, 0.0),
        ];
        let (_, steps) = run(mode(Mode::NotificationOnly), events);
        // Registering while the notice is up: no notification of our own.
        assert_eq!(steps[3], vec![Command::Register]);
        // The next dictation is notified normally.
        assert_eq!(
            steps[4],
            vec![Command::ShowNotification {
                message: Message::Loading,
                urgency: Urgency::Normal
            }]
        );
    }

    #[test]
    fn rn_novo_19_the_notice_is_shown_on_the_bar_without_a_notification_of_its_own() {
        let mut events = started(Mode::Both);
        events.push(reading(MynaState::Notice, Some("No speech detected"), 0.0));
        let (_, steps) = run(Config::default(), events);
        let last = steps.last().unwrap();
        assert!(last.contains(&look(
            BarKind::Notice,
            Some(Message::Notice("No speech detected".into()))
        )));
        assert!(
            !last
                .iter()
                .any(|c| matches!(c, Command::ShowNotification { .. }))
        );
    }

    #[test]
    fn rn_novo_8_error_reason_goes_to_the_bar_and_a_notification_in_both_mode() {
        let mut events = started(Mode::Both);
        events.push(reading(
            MynaState::Error,
            Some("No text field focused"),
            0.0,
        ));
        events.push(reading(MynaState::Idle, None, 0.0));
        let (_, steps) = run(Config::default(), events);
        let error = &steps[steps.len() - 2];
        assert!(error.contains(&look(
            BarKind::Error,
            Some(Message::Error("No text field focused".into()))
        )));
        assert!(error.contains(&Command::ShowNotification {
            message: Message::Error("No text field focused".into()),
            urgency: Urgency::Critical
        }));
        let idle = steps.last().unwrap();
        assert!(idle.contains(&Command::HideBar) && idle.contains(&Command::CloseNotification));
    }

    #[test]
    fn rn_novo_8_error_reason_stays_on_the_bar_only_in_bar_only_mode() {
        let mut events = started(Mode::BarOnly);
        events.push(reading(
            MynaState::Error,
            Some("No text field focused"),
            0.0,
        ));
        let (_, steps) = run(mode(Mode::BarOnly), events);
        assert!(
            !all(&steps)
                .iter()
                .any(|c| matches!(c, Command::ShowNotification { .. }))
        );
    }

    #[test]
    fn rn_novo_8_bar_failure_beats_bar_only_mode_and_the_error_goes_to_the_notification() {
        let mut events = started(Mode::BarOnly);
        events.push(Event::SurfaceFailed(Surface::Bar));
        events.push(Event::SurfaceReady(Surface::Notification));
        events.push(reading(
            MynaState::Error,
            Some("No text field focused"),
            0.0,
        ));
        let (_, steps) = run(mode(Mode::BarOnly), events);
        assert!(all(&steps).contains(&Command::StartSurface(Surface::Notification)));
        assert!(steps.last().unwrap().contains(&Command::ShowNotification {
            message: Message::Error("No text field focused".into()),
            urgency: Urgency::Critical
        }));
    }

    #[test]
    fn shutdown_unregisters_and_closes_everything_then_ignores_events() {
        let mut events = started(Mode::Both);
        events.push(reading(MynaState::Recording, None, 0.1));
        let (state, _) = run(Config::default(), events);
        let (state, commands) = decide(state, Event::Shutdown);
        assert!(commands.contains(&Command::Unregister));
        assert!(commands.contains(&Command::HideBar));
        let (_, after) = decide(state, reading(MynaState::Loading, None, 0.0));
        assert!(after.is_empty());
    }

    #[test]
    fn recording_updates_the_level_only_when_it_changes() {
        let mut events = started(Mode::Both);
        events.extend([
            reading(MynaState::Recording, None, 0.10),
            reading(MynaState::Recording, None, 0.10),
            reading(MynaState::Recording, None, 0.20),
        ]);
        let (_, steps) = run(Config::default(), events);
        let levels: Vec<_> = all(&steps)
            .into_iter()
            .filter_map(|c| {
                if let Command::UpdateLevel(level) = c {
                    Some(level)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(levels, vec![0.10, 0.20]);
    }
}
