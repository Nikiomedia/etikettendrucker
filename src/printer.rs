//! Bluetooth-LE-Verbindung zum Drucker. Läuft in einem eigenen Thread mit Tokio.
//!
//! Die Oberfläche schickt `Cmd`s und bekommt `Event`s zurück.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use btleplug::api::{Central, CentralEvent, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::StreamExt;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

use crate::protocol::{self, PrintJob, Response};

pub enum Cmd {
    /// Drucker suchen und verbinden. Der Gerätename `preferred` (zuletzt benutzt) wird bevorzugt.
    /// Gibt es mehrere passende Drucker, darf die Oberfläche wählen.
    Connect { preferred: Option<String> },
    /// Suchen und immer die Auswahlliste zeigen. Eine bestehende Verbindung bleibt,
    /// bis ein anderer Drucker gewählt wird.
    Choose,
    /// Aus der Auswahlliste: mit dem Drucker mit dieser Bluetooth-Adresse verbinden
    ConnectTo(String),
    /// Auswahl abbrechen
    CancelChoice,
    Disconnect,
    Print { job: PrintJob, copies: u32 },
    Battery,
}

/// Ein bei der Suche gefundener Drucker
#[derive(Clone, Debug, PartialEq)]
pub struct FoundPrinter {
    pub address: String,
    pub name: String,
    /// Signalstärke in dBm (näher an 0 = stärker)
    pub rssi: Option<i16>,
}

pub enum Event {
    Searching,
    /// Drucker zum Auswählen; wird während der Suche laufend aktualisiert
    Choices(Vec<FoundPrinter>),
    /// Auswahl beendet, ohne dass ein neuer Drucker verbunden wurde
    ChoiceClosed,
    Connecting(String),
    Connected(String),
    Disconnected,
    Battery(u8),
    /// Statusmeldung des Druckers (Papier, Deckel, …)
    Status(String),
    Progress { sent: usize, total: usize },
    PrintDone(Result<(), String>),
    Error(String),
    Log(String),
}

pub struct PrinterHandle {
    cmd_tx: UnboundedSender<Cmd>,
    ev_rx: mpsc::Receiver<Event>,
}

impl PrinterHandle {
    pub fn send(&self, cmd: Cmd) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn poll(&self) -> Option<Event> {
        self.ev_rx.try_recv().ok()
    }
}

#[derive(Clone)]
struct Emitter {
    tx: mpsc::Sender<Event>,
    repaint: Arc<dyn Fn() + Send + Sync>,
}

impl Emitter {
    fn send(&self, ev: Event) {
        let _ = self.tx.send(ev);
        (self.repaint)();
    }
    fn log(&self, s: impl Into<String>) {
        self.send(Event::Log(s.into()));
    }
}

pub fn spawn(repaint: impl Fn() + Send + Sync + 'static) -> PrinterHandle {
    let (cmd_tx, cmd_rx) = unbounded_channel();
    let (ev_tx, ev_rx) = mpsc::channel();
    let em = Emitter { tx: ev_tx, repaint: Arc::new(repaint) };
    std::thread::Builder::new()
        .name("bluetooth".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("Tokio-Laufzeit");
            rt.block_on(worker(cmd_rx, em));
        })
        .expect("Bluetooth-Thread");
    PrinterHandle { cmd_tx, ev_rx }
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

async fn worker(mut cmds: UnboundedReceiver<Cmd>, em: Emitter) {
    let manager = match Manager::new().await {
        Ok(m) => m,
        Err(e) => {
            em.send(Event::Error(format!("Bluetooth nicht verfügbar: {e}")));
            return;
        }
    };
    let mut conn: Option<Connection> = None;

    while let Some(cmd) = cmds.recv().await {
        match cmd {
            Cmd::Connect { preferred } => {
                if let Some(c) = conn.take() {
                    c.close().await;
                }
                connect_flow(&manager, &em, &mut cmds, &mut conn, preferred, false).await;
            }
            Cmd::Choose => {
                connect_flow(&manager, &em, &mut cmds, &mut conn, None, true).await;
            }
            // Nur während einer laufenden Suche von Bedeutung
            Cmd::ConnectTo(_) | Cmd::CancelChoice => {}
            Cmd::Disconnect => {
                if let Some(c) = conn.take() {
                    c.close().await;
                }
                em.send(Event::Disconnected);
            }
            Cmd::Print { job, copies } => {
                let result = match conn.as_mut() {
                    Some(c) if c.alive() => c.print(&job, copies.max(1), &em).await,
                    _ => Err("Kein Drucker verbunden.".into()),
                };
                em.send(Event::PrintDone(result));
                if let Some(c) = conn.as_mut().filter(|c| c.alive()) {
                    if let Some(level) = c.battery(&em).await {
                        em.send(Event::Battery(level));
                    }
                }
            }
            Cmd::Battery => {
                if let Some(c) = conn.as_mut().filter(|c| c.alive()) {
                    if let Some(level) = c.battery(&em).await {
                        em.send(Event::Battery(level));
                    }
                }
            }
        }
    }
}

/// Suchen, ggf. auswählen lassen, dann verbinden.
async fn connect_flow(
    manager: &Manager,
    em: &Emitter,
    cmds: &mut UnboundedReceiver<Cmd>,
    conn: &mut Option<Connection>,
    preferred: Option<String>,
    always_pick: bool,
) {
    // Jedes Mal ein frischer Adapter: so tauchen ausgeschaltete Drucker aus früheren Suchen nicht mehr auf
    let central = match first_adapter(manager).await {
        Ok(c) => c,
        Err(e) => return em.send(Event::Error(e)),
    };
    match discover(&central, em, cmds, preferred, always_pick).await {
        Ok(Some((periph, name))) => {
            if let Some(c) = conn.take() {
                c.close().await;
            }
            match Connection::open(&central, em, periph, name).await {
                Ok(mut c) => {
                    em.send(Event::Connected(c.name.clone()));
                    if let Some(level) = c.battery(em).await {
                        em.send(Event::Battery(level));
                    }
                    *conn = Some(c);
                }
                Err(e) => em.send(Event::Error(e)),
            }
        }
        Ok(None) => em.send(Event::ChoiceClosed),
        Err(e) => em.send(Event::Error(e)),
    }
}

/// Alle sichtbaren Drucker der P15-Familie, nach Name sortiert (damit die Liste nicht springt).
async fn scan_candidates(central: &Adapter, seen_any: &mut bool) -> Vec<(Peripheral, FoundPrinter)> {
    let mut out = Vec::new();
    for p in central.peripherals().await.unwrap_or_default() {
        if let Ok(Some(props)) = p.properties().await {
            if let Some(name) = props.local_name {
                *seen_any = true;
                if protocol::is_supported_name(&name) {
                    out.push((p, FoundPrinter { address: props.address.to_string(), name, rssi: props.rssi }));
                }
            }
        }
    }
    out.sort_by(|a, b| a.1.name.cmp(&b.1.name).then_with(|| a.1.address.cmp(&b.1.address)));
    out
}

fn not_found_message(seen_any: bool) -> String {
    if seen_any {
        "Kein Etikettendrucker gefunden. Ist er eingeschaltet? Er darf nicht gleichzeitig mit dem Handy \
         oder einem Browser-Tab verbunden sein."
            .into()
    } else {
        "Keine Bluetooth-Geräte gefunden. Ist Bluetooth am PC eingeschaltet?".into()
    }
}

/// Sucht Drucker.
///
/// - Automatisch: Der bevorzugte Drucker wird sofort genommen. Sonst wird kurz gewartet;
///   gibt es genau einen Drucker, wird er genommen, bei mehreren entscheidet die Oberfläche.
/// - `always_pick`: Die Oberfläche entscheidet immer.
///
/// `Ok(None)` heißt: abgebrochen.
async fn discover(
    central: &Adapter,
    em: &Emitter,
    cmds: &mut UnboundedReceiver<Cmd>,
    preferred: Option<String>,
    always_pick: bool,
) -> Result<Option<(Peripheral, String)>, String> {
    let mut picking = always_pick;
    if !picking {
        em.send(Event::Searching);
    }
    central
        .start_scan(ScanFilter::default())
        .await
        .map_err(|e| format!("Suche fehlgeschlagen (ist Bluetooth eingeschaltet?): {e}"))?;

    let start = Instant::now();
    let mut seen_any = false;
    let mut known: Vec<(Peripheral, FoundPrinter)> = Vec::new();
    let mut last_sent: Option<Vec<FoundPrinter>> = None;
    let mut tick = tokio::time::interval(Duration::from_millis(400));

    let result = loop {
        tokio::select! {
            _ = tick.tick() => {
                known = scan_candidates(central, &mut seen_any).await;
                let elapsed = start.elapsed();
                if !picking {
                    if let Some(pref) = &preferred {
                        if let Some((p, f)) = known.iter().find(|(_, f)| &f.name == pref) {
                            break Ok(Some((p.clone(), f.name.clone())));
                        }
                    }
                    // Kurz warten, ob noch weitere Drucker auftauchen
                    let grace = Duration::from_millis(if preferred.is_some() { 4000 } else { 2500 });
                    if elapsed >= grace {
                        match known.len() {
                            0 => {}
                            1 => break Ok(Some((known[0].0.clone(), known[0].1.name.clone()))),
                            n => {
                                em.log(format!("{n} Drucker gefunden – Auswahl anzeigen"));
                                picking = true;
                            }
                        }
                    }
                    if !picking && elapsed >= Duration::from_secs(12) {
                        break Err(not_found_message(seen_any));
                    }
                }
                if picking {
                    let list: Vec<FoundPrinter> = known.iter().map(|(_, f)| f.clone()).collect();
                    if last_sent.as_ref() != Some(&list) {
                        em.send(Event::Choices(list.clone()));
                        last_sent = Some(list);
                    }
                    if elapsed >= Duration::from_secs(120) {
                        em.log("Auswahl nach 2 Minuten ohne Entscheidung beendet");
                        break Ok(None);
                    }
                }
            }
            cmd = cmds.recv() => match cmd {
                Some(Cmd::ConnectTo(addr)) => {
                    if let Some((p, f)) = known.iter().find(|(_, f)| f.address == addr) {
                        break Ok(Some((p.clone(), f.name.clone())));
                    }
                    em.log("Der gewählte Drucker ist nicht mehr zu sehen");
                }
                Some(Cmd::CancelChoice) | None => break Ok(None),
                // Andere Befehle sind während der Suche nicht möglich
                Some(_) => {}
            }
        }
    };
    let _ = central.stop_scan().await;
    result
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Rx,
    Cx,
}

struct Shared {
    credits: AtomicI32,
    last_credit: Mutex<Instant>,
    packet_size: AtomicUsize,
    alive: AtomicBool,
}

struct Connection {
    name: String,
    periph: Peripheral,
    tx: Characteristic,
    shared: Arc<Shared>,
    msgs: UnboundedReceiver<(Source, Vec<u8>)>,
    tasks: Vec<JoinHandle<()>>,
}

async fn first_adapter(manager: &Manager) -> Result<Adapter, String> {
    manager
        .adapters()
        .await
        .map_err(|e| format!("Bluetooth nicht verfügbar: {e}"))?
        .into_iter()
        .next()
        .ok_or_else(|| "Kein Bluetooth-Adapter gefunden. Ist Bluetooth eingeschaltet?".to_string())
}

impl Connection {
    async fn open(central: &Adapter, em: &Emitter, periph: Peripheral, name: String) -> Result<Self, String> {
        em.send(Event::Connecting(name.clone()));
        let mut last_err = String::new();
        let mut ok = false;
        for attempt in 1..=4 {
            match periph.connect().await {
                Ok(()) => {
                    ok = true;
                    break;
                }
                Err(e) => {
                    last_err = e.to_string();
                    em.log(format!("Verbindungsversuch {attempt} fehlgeschlagen: {e}"));
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
            }
        }
        if !ok {
            return Err(format!("Verbindung zu {name} fehlgeschlagen: {last_err}"));
        }

        if let Err(e) = periph.discover_services().await {
            let _ = periph.disconnect().await;
            return Err(format!("Dienste des Druckers nicht lesbar: {e}"));
        }
        let chars = periph.characteristics();
        let find = |uuid| chars.iter().find(|c| c.uuid == uuid && c.service_uuid == protocol::SERVICE_UUID).cloned();
        let (Some(tx), Some(rx)) = (find(protocol::TX_UUID), find(protocol::RX_UUID)) else {
            let _ = periph.disconnect().await;
            return Err(format!("{name} spricht nicht das erwartete Protokoll (Dienst FF00 fehlt)."));
        };
        let cx = find(protocol::CX_UUID);

        let shared = Arc::new(Shared {
            credits: AtomicI32::new(0),
            last_credit: Mutex::new(Instant::now()),
            packet_size: AtomicUsize::new(protocol::PACKET_SIZE),
            alive: AtomicBool::new(true),
        });
        let (msg_tx, msgs) = unbounded_channel();

        // Erst den Benachrichtigungs-Strom holen, dann abonnieren – sonst gehen die ersten Credits verloren.
        let mut notifs = periph.notifications().await.map_err(|e| format!("Benachrichtigungen: {e}"))?;
        let mut tasks = Vec::new();
        {
            let shared = shared.clone();
            let em = em.clone();
            tasks.push(tokio::spawn(async move {
                while let Some(n) = notifs.next().await {
                    let src = if n.uuid == protocol::CX_UUID { Source::Cx } else { Source::Rx };
                    handle_notification(&shared, &em, &msg_tx, src, n.value);
                }
            }));
        }

        periph.subscribe(&rx).await.map_err(|e| format!("Abonnieren fehlgeschlagen: {e}"))?;
        if let Some(cx) = &cx {
            periph.subscribe(cx).await.map_err(|e| format!("Abonnieren fehlgeschlagen: {e}"))?;
        }

        // Trennung erkennen
        if let Ok(mut events) = central.events().await {
            let shared = shared.clone();
            let em = em.clone();
            let id = periph.id();
            tasks.push(tokio::spawn(async move {
                while let Some(ev) = events.next().await {
                    if let CentralEvent::DeviceDisconnected(d) = ev {
                        if d == id {
                            shared.alive.store(false, Ordering::SeqCst);
                            em.log("Drucker hat die Verbindung getrennt");
                            em.send(Event::Disconnected);
                            break;
                        }
                    }
                }
            }));
        }

        let mut conn = Connection { name, periph, tx, shared, msgs, tasks };

        // Auf die ersten Credits warten (max. 3 s)
        if cx.is_some() {
            let start = Instant::now();
            while conn.credits() <= 0 && start.elapsed() < Duration::from_secs(3) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            em.log(format!("Start-Credits: {}", conn.credits()));
        }
        conn.drain();
        Ok(conn)
    }

    fn alive(&self) -> bool {
        self.shared.alive.load(Ordering::SeqCst)
    }

    fn credits(&self) -> i32 {
        self.shared.credits.load(Ordering::SeqCst)
    }

    fn drain(&mut self) {
        while self.msgs.try_recv().is_ok() {}
    }

    async fn close(mut self) {
        let _ = self.periph.disconnect().await;
        for t in self.tasks.drain(..) {
            t.abort();
        }
    }

    /// Sendet Daten paketweise mit Credit-Flusssteuerung, genau wie die Original-App:
    /// alle 30 ms ein Paket, solange Credits da sind.
    async fn send(&self, data: &[u8], mut progress: impl FnMut(usize)) -> Result<(), String> {
        let tick = Duration::from_millis(protocol::PACKET_DELAY_MS);
        *self.shared.last_credit.lock().unwrap() = Instant::now();
        let mut offset = 0;
        while offset < data.len() {
            if !self.alive() {
                return Err("Verbindung zum Drucker verloren.".into());
            }
            // Nach 1 s ohne Credits einen erzwingen
            if self.credits() <= 0 {
                let mut last = self.shared.last_credit.lock().unwrap();
                if last.elapsed() >= Duration::from_secs(1) {
                    self.shared.credits.store(1, Ordering::SeqCst);
                    *last = Instant::now();
                }
            }
            if self.credits() > 0 {
                let size = self.shared.packet_size.load(Ordering::SeqCst).min(data.len() - offset);
                let chunk = &data[offset..offset + size];
                if let Err(e) = self.periph.write(&self.tx, chunk, WriteType::WithoutResponse).await {
                    // Evtl. zu kleine MTU: auf 20 Byte gehen und nochmal
                    if size > 20 {
                        self.shared.packet_size.store(20, Ordering::SeqCst);
                        continue;
                    }
                    return Err(format!("Senden fehlgeschlagen: {e}"));
                }
                self.shared.credits.fetch_sub(1, Ordering::SeqCst);
                offset += size;
                progress(offset);
                if offset >= data.len() {
                    break;
                }
            }
            tokio::time::sleep(tick).await;
        }
        Ok(())
    }

    async fn wait_credits(&self, min: i32, timeout: Duration) {
        let start = Instant::now();
        while self.credits() < min && start.elapsed() < timeout {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn wait_print_result(&mut self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            if !self.alive() {
                // Drucker hat sich nach dem Druck abgeschaltet – Daten waren schon komplett gesendet
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("Keine Rückmeldung vom Drucker (Zeitüberschreitung).".into());
            }
            match tokio::time::timeout(Duration::from_millis(200), self.msgs.recv()).await {
                Ok(Some((_, data))) => match protocol::parse_response(&data) {
                    Some(Response::Success) => return Ok(()),
                    Some(Response::Status(code)) if protocol::status_is_fatal(code) => {
                        return Err(protocol::status_text(code));
                    }
                    _ => {}
                },
                Ok(None) => return Err("Verbindung zum Drucker verloren.".into()),
                Err(_) => {}
            }
        }
    }

    async fn print(&mut self, job: &PrintJob, copies: u32, em: &Emitter) -> Result<(), String> {
        let single = job.preamble.len() + job.bulk.len();
        let total = single * copies as usize;
        em.log(format!(
            "Druck: {} Byte Bild, {} Kopie(n), Paketgröße {}",
            job.bulk.len(),
            copies,
            self.shared.packet_size.load(Ordering::SeqCst)
        ));
        for copy in 0..copies as usize {
            self.drain();
            let base = copy * single;
            self.send(&job.preamble, |_| {}).await?;
            // Drucker verarbeitet den Vorspann und gibt Credits zurück
            self.wait_credits(3, Duration::from_secs(1)).await;
            let pre = job.preamble.len();
            self.send(&job.bulk, |sent| em.send(Event::Progress { sent: base + pre + sent, total }))
                .await?;
            self.wait_print_result(Duration::from_secs(30)).await?;
            em.log(format!("Etikett {} von {copies} fertig", copy + 1));
        }
        Ok(())
    }

    async fn battery(&mut self, em: &Emitter) -> Option<u8> {
        self.drain();
        self.send(&protocol::CMD_BATTERY, |_| {}).await.ok()?;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(200), self.msgs.recv()).await {
                // Antworten, die kein bekanntes Format haben, sind die Akku-Antwort (wie in thermoprint)
                Ok(Some((Source::Rx, data))) if protocol::parse_response(&data).is_none() => {
                    em.log(format!("Akku-Antwort: {}", hex(&data)));
                    let level = if data.len() >= 2 { data[1] } else { *data.first()? };
                    return (level <= 100).then_some(level);
                }
                Ok(None) => return None,
                _ => {}
            }
        }
        None
    }
}

fn handle_notification(
    shared: &Shared,
    em: &Emitter,
    msg_tx: &UnboundedSender<(Source, Vec<u8>)>,
    src: Source,
    data: Vec<u8>,
) {
    let parsed = protocol::parse_response(&data);
    match (src, parsed) {
        (Source::Cx, Some(Response::Credit(n))) => {
            shared.credits.fetch_add(n as i32, Ordering::SeqCst);
            *shared.last_credit.lock().unwrap() = Instant::now();
            return;
        }
        (Source::Cx, Some(Response::Mtu(mtu))) => {
            if mtu > 3 {
                let size = (mtu as usize - 3).min(protocol::PACKET_SIZE);
                shared.packet_size.store(size, Ordering::SeqCst);
                em.log(format!("MTU {mtu} → Paketgröße {size}"));
            }
            return;
        }
        (Source::Cx, Some(Response::Success)) => {}
        (Source::Cx, _) => return,
        (Source::Rx, Some(Response::Status(code))) => {
            em.send(Event::Status(protocol::status_text(code)));
        }
        _ => {}
    }
    em.log(format!("{} ← {}", if src == Source::Rx { "RX" } else { "CX" }, hex(&data)));
    let _ = msg_tx.send((src, data));
}
