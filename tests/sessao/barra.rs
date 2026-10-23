//! Session tests: the bar.
//!
//! Every test here is `#[ignore]` and runs by hand in a COSMIC session with
//! `cargo test --test sessao -- --ignored --nocapture barra`.

use std::{
    sync::{Arc, Mutex},
    thread::sleep,
    time::{Duration, Instant},
};

use myna_layout::{
    core::{Bar, BarKind, BarLook, Config, Event, Message, Surface},
    surface::bar::{BarState, Connected},
};

/// CEN-NOVO-15 and CEN-NOVO-16: the bar shows without taking the focus and goes away.
///
/// The test shows the bar for 15 seconds while the level moves. **While it is up,
/// click into a text field of another application and type or dictate with Myna: the
/// text must keep arriving in that field** (the bar must not steal the focus, and
/// clicking on it must hit what is behind it). When the test ends the bar must
/// disappear.
#[test]
#[ignore = "needs a COSMIC session; see tests/sessao/README.md"]
fn cen_novo_15_the_bar_shows_without_taking_the_focus_and_cen_novo_16_goes_away() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = {
        let events = Arc::clone(&events);
        Box::new(move |event: Event| events.lock().unwrap().push(event))
    };
    let Connected {
        mut state,
        mut queue,
        ..
    } = BarState::connect(sink, Config::default()).expect("could not connect to the compositor");

    state.start();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !events
        .lock()
        .unwrap()
        .contains(&Event::SurfaceReady(Surface::Bar))
    {
        assert!(
            Instant::now() < deadline,
            "the compositor never configured the bar"
        );
        queue.roundtrip(&mut state).expect("dispatch");
        sleep(Duration::from_millis(20));
    }

    state.show(&BarLook {
        kind: BarKind::Recording,
        message: Some(Message::Listening),
    });
    println!("The bar is up for 15 s: click into a text field elsewhere and type or dictate.");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(15) {
        let seconds = start.elapsed().as_secs_f32();
        state.update_level(0.1 + 0.08 * (seconds * 3.0).sin());
        queue.roundtrip(&mut state).expect("dispatch");
        sleep(Duration::from_millis(40));
    }

    state.hide();
    queue.roundtrip(&mut state).expect("dispatch");
    println!("The bar must be gone now.");
    sleep(Duration::from_secs(2));

    assert!(
        !events
            .lock()
            .unwrap()
            .contains(&Event::SurfaceFailed(Surface::Bar))
    );
}
