//! Manual notification checks in a real COSMIC session.

use std::{
    sync::{Arc, Mutex},
    thread::sleep,
    time::{Duration, Instant},
};

use myna_layout::{
    core::{Event, Message, Notifier, OneTimeNotice, Surface, Urgency},
    i18n::Language,
    surface::notify::DbusNotifier,
};

#[test]
#[ignore = "requires a COSMIC session with a freedesktop notification service"]
fn notification_service_is_ready_and_displays_normal_critical_and_one_time_notices() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = {
        let events = Arc::clone(&events);
        Box::new(move |event| events.lock().unwrap().push(event))
    };
    let mut notifier = DbusNotifier::connect(None, Language::from_env(), sink)
        .expect("connect to the user's session bus");
    notifier.start();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let current = events.lock().unwrap();
        if current.contains(&Event::SurfaceReady(Surface::Notification)) {
            break;
        }
        assert!(
            !current.contains(&Event::SurfaceFailed(Surface::Notification)),
            "notification service failed to start: {current:?}"
        );
        assert!(
            Instant::now() < deadline,
            "notification service did not respond"
        );
        drop(current);
        sleep(Duration::from_millis(20));
    }

    notifier.show(&Message::Listening, Urgency::Normal);
    sleep(Duration::from_secs(1));
    notifier.show(
        &Message::Error("No text field focused".into()),
        Urgency::Critical,
    );
    sleep(Duration::from_secs(1));
    notifier.notice_once(OneTimeNotice::MynaUnavailable);
    sleep(Duration::from_secs(1));
    notifier.close();
}
