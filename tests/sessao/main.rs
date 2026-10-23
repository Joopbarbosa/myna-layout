//! Session test target: every test is `#[ignore]` and runs by hand in a COSMIC
//! session with `cargo test --test sessao -- --ignored`. Each task fills only its
//! own file.

mod animacao;
mod barra;
mod instalacao;
mod notificacao;
mod ponta_a_ponta;
