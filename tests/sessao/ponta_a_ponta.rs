//! Session tests: the whole program in a real COSMIC session.
//!
//! Every test here is `#[ignore]` and runs by hand in a COSMIC session with the real
//! Myna: `cargo test --test sessao -- --ignored --nocapture ponta_a_ponta`.

use std::{
    process::{Command, Stdio},
    thread::sleep,
    time::Duration,
};

/// CEN-NOVO-1 and CEN-NOVO-12, by hand: the indicator runs for 60 seconds on the real
/// session bus and the real compositor. Dictate with Myna (your usual shortcut) into a
/// text field while it runs: the bar must show Loading, Listening and Finishing and
/// disappear when Myna is idle again, and the text must keep arriving in the field.
/// To see the fallback, kill the bar's Wayland socket is not needed: unset
/// `WAYLAND_DISPLAY` for a second run and the notification takes over.
#[test]
#[ignore = "needs a COSMIC session and the real Myna; see tests/sessao/README.md"]
fn cen_novo_1_the_indicator_follows_a_real_dictation() {
    let mut program = Command::new(env!("CARGO_BIN_EXE_myna-layout"))
        .env_remove("MYNA_LAYOUT_BUS_ADDRESS")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("the program must start");
    println!("myna-layout is running for 60 s: dictate with Myna and watch the bar.");
    sleep(Duration::from_secs(60));
    Command::new("kill")
        .args(["-TERM", &program.id().to_string()])
        .status()
        .expect("kill");
    let status = program.wait().expect("wait");
    assert!(
        status.success(),
        "the program must exit cleanly on SIGTERM, got {status}"
    );
}
