//! Session tests: instalacao.
//!
//! Every test here is `#[ignore]` and runs by hand in a COSMIC session with
//! `cargo test --test sessao -- --ignored`. Name each test after its scenario.

#[test]
#[ignore = "requires manual package installation and a COSMIC login"]
fn cen_novo_20_21_29_package_install_menu_and_removal_manual_steps() {
    println!(
        "Manual CEN-NOVO-20/21/29 checklist:\n\
         1. Install the built package with: sudo apt install ./myna-layout_*.deb\n\
         2. Log out and back in; confirm myna-layout.service is active and the indicator works during dictation.\n\
         3. Open the Myna Layout menu entry twice; confirm only one service/process instance runs.\n\
         4. Remove the package normally; confirm the service stops and ~/.config/myna-layout is preserved.\n\
         5. Reinstall, then purge the package; confirm only ~/.config/myna-layout is removed.\n\
         This test does not run sudo or install/remove anything."
    );
}
