//! Desktop notification surface backed by the freedesktop notification service.

use std::{collections::HashMap, fmt, thread, time::Duration};

use crate::{
    core::{Event, Message, Notifier, OneTimeNotice, Surface, Urgency},
    i18n::{self, Language},
};
use zbus::{Connection, OwnedVariant};

const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const INTERFACE: &str = "org.freedesktop.Notifications";

/// Why the notification surface could not start.
#[derive(Debug)]
pub enum NotifyError {
    Bus(zbus::Error),
    WorkerUnavailable,
}

impl fmt::Display for NotifyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bus(error) => write!(formatter, "could not connect to notification bus: {error}"),
            Self::WorkerUnavailable => formatter.write_str("notification worker did not start"),
        }
    }
}

impl std::error::Error for NotifyError {}

enum Request {
    Start,
    Show(String, Urgency),
    Notice(String),
    Close,
    CloseNotice(u32),
    Flush(std::sync::mpsc::Sender<()>),
}

/// Asynchronous adapter for `org.freedesktop.Notifications`.
pub struct DbusNotifier {
    requests: async_channel::Sender<Request>,
    language: Language,
}

impl DbusNotifier {
    /// Connect to the session bus, or to a private bus address used by integration tests.
    pub fn connect(
        bus_address: Option<&str>,
        language: Language,
        sink: Box<dyn Fn(Event) + Send>,
    ) -> Result<Self, NotifyError> {
        let address = bus_address.map(str::to_owned);
        let connection = zbus::block_on(async {
            match &address {
                Some(address) => {
                    zbus::connection::Builder::address(address.as_str())
                        .build()
                        .await
                }
                None => zbus::connection::Builder::session().build().await,
            }
        })
        .map_err(NotifyError::Bus)?;
        let (requests, receiver) = async_channel::unbounded();
        let worker_sender = requests.clone();
        thread::Builder::new()
            .name("notification-dbus".into())
            .spawn(move || {
                if let Err(error) =
                    zbus::block_on(worker(connection, receiver, worker_sender, language, sink))
                {
                    tracing::error!(%error, "notification worker stopped");
                }
            })
            .map_err(|_| NotifyError::WorkerUnavailable)?;
        Ok(Self { requests, language })
    }

    fn send(&self, request: Request) {
        if self.requests.try_send(request).is_err() {
            tracing::error!("notification worker stopped");
        }
    }
}

impl Notifier for DbusNotifier {
    fn start(&mut self) {
        self.send(Request::Start);
    }

    fn show(&mut self, message: &Message, urgency: Urgency) {
        self.send(Request::Show(i18n::text(message, self.language), urgency));
    }

    fn notice_once(&mut self, notice: OneTimeNotice) {
        self.send(Request::Notice(i18n::one_time_text(notice, self.language)));
    }

    fn close(&mut self) {
        self.send(Request::Close);
    }

    fn flush(&mut self, timeout: Duration) {
        let (done, wait) = std::sync::mpsc::channel();
        self.send(Request::Flush(done));
        let _ = wait.recv_timeout(timeout);
    }
}

async fn worker(
    connection: Connection,
    requests: async_channel::Receiver<Request>,
    sender: async_channel::Sender<Request>,
    _language: Language,
    sink: Box<dyn Fn(Event) + Send>,
) -> zbus::Result<()> {
    let mut current_id = 0;
    while let Ok(request) = requests.recv().await {
        match request {
            Request::Start => {
                let result = async {
                    let proxy = zbus::Proxy::new(&connection, SERVICE, PATH, INTERFACE).await?;
                    proxy
                        .call::<_, _, (String, String, String, String)>("GetServerInformation", &())
                        .await
                }
                .await;
                match result {
                    Ok(_) => sink(Event::SurfaceReady(Surface::Notification)),
                    Err(error) => {
                        tracing::warn!(%error, "notification service is unavailable");
                        sink(Event::SurfaceFailed(Surface::Notification));
                    }
                }
            }
            Request::Show(summary, urgency) => {
                match notify(&connection, current_id, &summary, urgency).await {
                    Ok(id) => {
                        if current_id != 0
                            && id != current_id
                            && let Err(error) = close_id(&connection, current_id).await
                        {
                            tracing::warn!(%error, "replaced notification close failed");
                        }
                        current_id = id;
                    }
                    Err(error) => {
                        tracing::error!(%error, "notification show failed");
                        sink(Event::SurfaceFailed(Surface::Notification));
                    }
                }
            }
            Request::Notice(summary) => {
                match notify(&connection, 0, &summary, Urgency::Normal).await {
                    Ok(id) => schedule_notice_close(sender.clone(), id),
                    Err(error) => {
                        tracing::error!(%error, "one-time notification failed");
                        sink(Event::SurfaceFailed(Surface::Notification));
                    }
                }
            }
            Request::Close => {
                if current_id != 0 {
                    if let Err(error) = close_id(&connection, current_id).await {
                        tracing::warn!(%error, "notification close failed");
                    }
                    current_id = 0;
                }
            }
            Request::Flush(done) => {
                let _ = done.send(());
            }
            Request::CloseNotice(id) => {
                if let Err(error) = close_id(&connection, id).await {
                    tracing::warn!(%error, "one-time notification close failed");
                }
            }
        }
    }
    Ok(())
}

fn schedule_notice_close(sender: async_channel::Sender<Request>, id: u32) {
    let _ = thread::Builder::new()
        .name("notification-expiry".into())
        .spawn(move || {
            thread::sleep(Duration::from_secs(8));
            let _ = sender.try_send(Request::CloseNotice(id));
        });
}

async fn notify(
    connection: &Connection,
    replaces_id: u32,
    summary: &str,
    urgency: Urgency,
) -> zbus::Result<u32> {
    let mut hints = HashMap::<&str, OwnedVariant>::new();
    hints.insert(
        "urgency",
        OwnedVariant::from(match urgency {
            Urgency::Normal => 1u8,
            Urgency::Critical => 2u8,
        }),
    );
    hints.insert("transient", OwnedVariant::from(true));
    let proxy = zbus::Proxy::new(connection, SERVICE, PATH, INTERFACE).await?;
    proxy
        .call::<_, _, u32>(
            "Notify",
            &(
                i18n::app_name(),
                replaces_id,
                "",
                summary,
                "",
                Vec::<String>::new(),
                hints,
                0i32,
            ),
        )
        .await
}

async fn close_id(connection: &Connection, id: u32) -> zbus::Result<()> {
    let proxy = zbus::Proxy::new(connection, SERVICE, PATH, INTERFACE).await?;
    proxy.call::<_, _, ()>("CloseNotification", &(id,)).await
}
