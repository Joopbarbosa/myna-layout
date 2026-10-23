#![allow(dead_code)]

//! Test support for a private fake Myna D-Bus service.
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    struct WakeThread(thread::Thread);
    impl std::task::Wake for WakeThread {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.unpark();
        }
        fn wake_by_ref(self: &std::sync::Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(std::sync::Arc::new(WakeThread(thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(output) => return output,
            std::task::Poll::Pending => thread::park(),
        }
    }
}

const SERVICE: &str = "com.canonical.Myna.Dictation";
const PATH: &str = "/com/canonical/Myna/Dictation";
const CONTROL: &str = "org.mynalayout.FakeControl";

/// Owns a private session bus and the fake service connected to it.
pub struct FakeMyna {
    address: String,
    daemon: Child,
    service: Child,
}
pub struct TestClient {
    connection: zbus::Connection,
}
impl TestClient {
    pub fn register(address: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let connection = block_on(zbus::connection::Builder::address(address).build())?;
        let proxy = block_on(zbus::Proxy::new(&connection, SERVICE, PATH, SERVICE))?;
        let _: u32 = block_on(proxy.call("RegisterClient", &()))?;
        Ok(Self { connection })
    }
}
impl FakeMyna {
    /// Starts an isolated D-Bus daemon and the fake service.
    pub fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut line = String::new();
        BufReader::new(daemon.stdout.take().ok_or("missing bus output")?).read_line(&mut line)?;
        let address = line.trim().to_owned();
        if address.is_empty() {
            let stderr = daemon
                .stderr
                .take()
                .map(|mut stream| {
                    use std::io::Read;
                    let mut text = String::new();
                    let _ = stream.read_to_string(&mut text);
                    text
                })
                .unwrap_or_default();
            let _ = daemon.wait();
            return Err(format!("dbus-daemon failed to start private bus: {stderr}").into());
        }
        let service = Command::new(env!("CARGO_BIN_EXE_fake-myna"))
            .env("FAKE_MYNA_BUS_ADDRESS", &address)
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let mut fake = Self {
            address,
            daemon,
            service,
        };
        for _ in 0..50 {
            if fake.service.try_wait()?.is_some() {
                return Err("fake-myna exited during startup".into());
            }
            if fake.registered_clients().is_ok() {
                return Ok(fake);
            }
            thread::sleep(Duration::from_millis(20));
        }
        Err("fake-myna did not become ready".into())
    }
    /// Returns the private bus address.
    pub fn bus_address(&self) -> &str {
        &self.address
    }
    fn call(
        &self,
        interface: &str,
        method: &str,
        args: &[&str],
    ) -> Result<String, Box<dyn std::error::Error>> {
        let output = Command::new("gdbus")
            .arg("call")
            .arg("--address")
            .arg(&self.address)
            .arg("--dest")
            .arg(SERVICE)
            .arg("--object-path")
            .arg(PATH)
            .arg("--method")
            .arg(format!("{interface}.{method}"))
            .args(args)
            .output()?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string().into());
        }
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
    fn property(&self, interface: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
        self.call("org.freedesktop.DBus.Properties", "Get", &[interface, name])
    }
    /// Sets the simulated Myna state.
    pub fn set_state(
        &self,
        state: &str,
        text: &str,
        level: f64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.call(CONTROL, "SetState", &[state, text, &level.to_string()])?;
        Ok(())
    }
    /// Sets the simulated audio level.
    pub fn set_level(&self, level: f64) -> Result<(), Box<dyn std::error::Error>> {
        self.call(CONTROL, "SetLevel", &[&level.to_string()])?;
        Ok(())
    }
    /// Simulates a service restart.
    pub fn restart(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.call(CONTROL, "Restart", &[])?;
        Ok(())
    }
    /// Returns how many unique clients are registered.
    pub fn registered_clients(&self) -> Result<u32, Box<dyn std::error::Error>> {
        parse_u32(&self.property(CONTROL, "RegisteredClients")?)
    }
    /// Returns how many Toggle calls were received.
    pub fn toggle_calls(&self) -> Result<u32, Box<dyn std::error::Error>> {
        parse_u32(&self.property(CONTROL, "ToggleCalls")?)
    }
}
/// Reads the number out of a `gdbus` reply such as `(<uint32 2>,)`.
fn parse_u32(reply: &str) -> Result<u32, Box<dyn std::error::Error>> {
    let digits: String = reply
        .split("uint32 ")
        .nth(1)
        .ok_or_else(|| format!("no uint32 in reply: {reply}"))?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    Ok(digits.parse()?)
}

impl Drop for FakeMyna {
    fn drop(&mut self) {
        let _ = self.service.kill();
        let _ = self.service.wait();
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_missing_private_bus_address() {
        let output = Command::new(env!("CARGO_BIN_EXE_fake-myna"))
            .env_remove("FAKE_MYNA_BUS_ADDRESS")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
    #[test]
    fn publishes_state_and_audio_properties() {
        let fake = FakeMyna::start()
            .expect("private dbus-daemon must start for fake-myna integration tests");
        fake.set_state("recording", "Listening", 0.4).unwrap();
        assert!(
            fake.property(SERVICE, "State")
                .unwrap()
                .contains("recording")
        );
        assert!(fake.property(SERVICE, "AudioRms").unwrap().contains("0.4"));
    }
    #[test]
    fn counts_toggle_calls() {
        let fake = FakeMyna::start()
            .expect("private dbus-daemon must start for fake-myna integration tests");
        fake.call(SERVICE, "Toggle", &[]).unwrap();
        assert_eq!(fake.toggle_calls().unwrap(), 1);
    }
    #[test]
    fn registration_is_idempotent_and_suppresses_toasts() {
        let fake = FakeMyna::start()
            .expect("private dbus-daemon must start for fake-myna integration tests");
        let client = TestClient::register(fake.bus_address()).unwrap();
        let proxy = block_on(zbus::Proxy::new(&client.connection, SERVICE, PATH, SERVICE)).unwrap();
        let _: u32 = block_on(proxy.call("RegisterClient", &())).unwrap();
        assert_eq!(fake.registered_clients().unwrap(), 1);
        assert!(
            fake.property(CONTROL, "ToastSuppressed")
                .unwrap()
                .contains("true")
        );
    }
    #[test]
    fn registration_clears_after_client_disconnect() {
        let fake = FakeMyna::start()
            .expect("private dbus-daemon must start for fake-myna integration tests");
        let client = TestClient::register(fake.bus_address()).unwrap();
        assert_eq!(fake.registered_clients().unwrap(), 1);
        drop(client);
        for _ in 0..50 {
            if fake.registered_clients().unwrap() == 0 {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("RegisteredClients did not clear after client disconnect");
    }
    #[test]
    fn restart_clears_clients_and_reacquires_name() {
        let fake = FakeMyna::start()
            .expect("private dbus-daemon must start for fake-myna integration tests");
        let _client = TestClient::register(fake.bus_address()).unwrap();
        fake.restart().unwrap();
        assert_eq!(fake.registered_clients().unwrap(), 0);
        assert!(
            fake.property(SERVICE, "State").is_ok(),
            "service name was not reacquired"
        );
    }
}
