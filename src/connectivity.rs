// garlemald-client — cross-platform launcher for FINAL FANTASY XIV 1.x private servers
// Copyright (C) 2026  Samuel Stegall
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Pre-flight connectivity probes for the selected game server.
//!
//! The launcher's auth step goes over HTTPS, but the *game* connects to
//! the server's lobby (54994) and world (54992) TCP listeners — and when
//! one of those is unreachable from a player's network, the server sees
//! an auth hit with no lobby follow-up and the player sees a silent hang.
//! During the 2026-08 bahamut net test that failure class was
//! indistinguishable (server-side) from a wrong password or a client that
//! never launched; a per-port probe from the player's machine is what
//! disambiguates it.
//!
//! Outcomes are classified the way a human would triage them:
//! - **Open** — a TCP connect succeeded (the service answered the door);
//! - **Refused** — the host answered with RST: the machine is reachable
//!   but nothing listens there (server down / wrong port);
//! - **TimedOut** — no answer at all: a firewall on either end, or an
//!   unforwarded port, is eating the SYN;
//! - **ResolveFailed** — DNS never produced an address to try.
//!
//! The map port (1989) is probed as *advisory only*: garlemald-server
//! topology has the game dial the map service directly, but other server
//! implementations (e.g. the bahamut C++ stack) relay map traffic behind
//! the world listener and never expose 1989 — a filtered map port there
//! is normal, not a fault. The verdict logic therefore only fails a
//! server on lobby/world/auth reachability.

use std::fmt;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// The lobby port the 1.x client dials first (hardcoded in the game PE;
/// the launcher patches only the hostname).
pub const LOBBY_PORT: u16 = 54994;
/// The world port the lobby hands the client to after character select.
pub const WORLD_PORT: u16 = 54992;
/// The map/zone port used by direct-dial server topologies
/// (garlemald-server). Advisory: relay topologies never expose it.
pub const MAP_PORT: u16 = 1989;

/// How one probed endpoint answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// TCP connect succeeded within the timeout.
    Open,
    /// The host actively refused (RST) — reachable, but not listening.
    Refused,
    /// No response within the timeout — filtered/unforwarded/unreachable.
    TimedOut,
    /// The server name never resolved to an address.
    ResolveFailed(String),
    /// Any other socket error (no route, network down, …).
    Error(String),
}

impl ProbeOutcome {
    pub fn is_open(&self) -> bool {
        matches!(self, ProbeOutcome::Open)
    }
}

impl fmt::Display for ProbeOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProbeOutcome::Open => write!(f, "open"),
            ProbeOutcome::Refused => write!(f, "refused (host up, nothing listening)"),
            ProbeOutcome::TimedOut => write!(f, "timed out (blocked or unforwarded)"),
            ProbeOutcome::ResolveFailed(e) => write!(f, "DNS resolution failed: {e}"),
            ProbeOutcome::Error(e) => write!(f, "error: {e}"),
        }
    }
}

/// The role a probed port plays, for reporting and verdict weighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortRole {
    Lobby,
    World,
    /// Advisory: unreachable is normal on relay-topology servers.
    Map,
}

impl fmt::Display for PortRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PortRole::Lobby => write!(f, "lobby"),
            PortRole::World => write!(f, "world"),
            PortRole::Map => write!(f, "map"),
        }
    }
}

/// One probed endpoint's result.
#[derive(Debug, Clone)]
pub struct PortProbe {
    pub role: PortRole,
    pub port: u16,
    pub outcome: ProbeOutcome,
    /// Wall time the probe took (round-trip on success, the full timeout
    /// on `TimedOut`).
    pub elapsed: Duration,
}

/// A full pre-flight report for one server address.
#[derive(Debug, Clone)]
pub struct ConnectivityReport {
    pub address: String,
    pub probes: Vec<PortProbe>,
}

impl ConnectivityReport {
    /// True when every port the game *requires* answered: lobby and
    /// world. The map probe is advisory (see module docs) and never
    /// fails the verdict.
    pub fn game_ports_ok(&self) -> bool {
        self.probes
            .iter()
            .filter(|p| matches!(p.role, PortRole::Lobby | PortRole::World))
            .all(|p| p.outcome.is_open())
    }

    /// One-line human summary, for logs and the GUI status row.
    pub fn summary(&self) -> String {
        let parts: Vec<String> = self
            .probes
            .iter()
            .map(|p| format!("{} {}: {}", p.role, p.port, p.outcome))
            .collect();
        format!("{} — {}", self.address, parts.join("; "))
    }
}

/// Classify a connect error the way the triage taxonomy wants it.
fn classify_error(err: &std::io::Error) -> ProbeOutcome {
    use std::io::ErrorKind;
    match err.kind() {
        ErrorKind::ConnectionRefused => ProbeOutcome::Refused,
        ErrorKind::TimedOut | ErrorKind::WouldBlock => ProbeOutcome::TimedOut,
        _ => ProbeOutcome::Error(err.to_string()),
    }
}

/// Probe one `host:port` with a bounded connect.
fn probe_port(host: &str, role: PortRole, port: u16, timeout: Duration) -> PortProbe {
    let started = Instant::now();
    let addrs: Vec<SocketAddr> = match (host, port).to_socket_addrs() {
        Ok(it) => it.collect(),
        Err(e) => {
            return PortProbe {
                role,
                port,
                outcome: ProbeOutcome::ResolveFailed(e.to_string()),
                elapsed: started.elapsed(),
            };
        }
    };
    let Some(addr) = addrs.first() else {
        return PortProbe {
            role,
            port,
            outcome: ProbeOutcome::ResolveFailed("no addresses returned".to_string()),
            elapsed: started.elapsed(),
        };
    };
    let outcome = match TcpStream::connect_timeout(addr, timeout) {
        Ok(_stream) => ProbeOutcome::Open,
        Err(e) => classify_error(&e),
    };
    PortProbe {
        role,
        port,
        outcome,
        elapsed: started.elapsed(),
    }
}

/// Probe the three game ports of `address`, in parallel so the
/// worst-case wall time is one `timeout`, not three. Blocking — run on a
/// worker thread (or through [`ConnectivityTask`]).
pub fn probe_server(address: &str, timeout: Duration) -> ConnectivityReport {
    let roles = [
        (PortRole::Lobby, LOBBY_PORT),
        (PortRole::World, WORLD_PORT),
        (PortRole::Map, MAP_PORT),
    ];
    let probes = std::thread::scope(|scope| {
        let handles: Vec<_> = roles
            .iter()
            .map(|&(role, port)| scope.spawn(move || probe_port(address, role, port, timeout)))
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join().unwrap_or_else(|_| PortProbe {
                    role: PortRole::Lobby,
                    port: 0,
                    outcome: ProbeOutcome::Error("probe thread panicked".to_string()),
                    elapsed: Duration::ZERO,
                })
            })
            .collect()
    });
    ConnectivityReport {
        address: address.to_string(),
        probes,
    }
}

/// A connectivity probe running on a background thread, polled from the
/// GUI loop — the same shape as `login::LoginTask` (spawn once, then
/// `try_recv` every frame until the result lands).
pub struct ConnectivityTask {
    rx: std::sync::mpsc::Receiver<ConnectivityReport>,
    /// Whether the GUI should surface a success as an info message
    /// (a user-initiated check) or stay quiet about it (an automatic
    /// pre-launch probe, where only failures matter).
    pub announce_success: bool,
}

impl ConnectivityTask {
    pub fn spawn(address: String, timeout: Duration, announce_success: bool) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("connectivity-probe".to_string())
            .spawn(move || {
                // A dropped receiver (task discarded mid-probe) is fine.
                let _ = tx.send(probe_server(&address, timeout));
            })
            .expect("spawning the connectivity probe thread");
        Self {
            rx,
            announce_success,
        }
    }

    /// Non-blocking: `Some(report)` exactly once, when the probe finishes.
    pub fn try_recv(&mut self) -> Option<ConnectivityReport> {
        self.rx.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    const FAST: Duration = Duration::from_millis(2_000);

    #[test]
    fn open_port_reports_open() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let port = listener.local_addr().unwrap().port();
        let probe = probe_port("127.0.0.1", PortRole::Lobby, port, FAST);
        assert_eq!(probe.outcome, ProbeOutcome::Open);
    }

    #[test]
    fn closed_port_reports_refused() {
        // Bind-then-drop guarantees the port exists but nothing listens,
        // so loopback answers with an immediate RST on every platform.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let probe = probe_port("127.0.0.1", PortRole::World, port, FAST);
        assert_eq!(probe.outcome, ProbeOutcome::Refused);
    }

    #[test]
    fn unresolvable_host_reports_resolve_failed() {
        // RFC 6761 reserves `.invalid` to never resolve.
        let probe = probe_port("garlemald.invalid", PortRole::Lobby, LOBBY_PORT, FAST);
        assert!(matches!(probe.outcome, ProbeOutcome::ResolveFailed(_)));
    }

    #[test]
    fn verdict_requires_lobby_and_world_but_not_map() {
        let mk = |role, outcome| PortProbe {
            role,
            port: 0,
            outcome,
            elapsed: Duration::ZERO,
        };
        let report = ConnectivityReport {
            address: "test".into(),
            probes: vec![
                mk(PortRole::Lobby, ProbeOutcome::Open),
                mk(PortRole::World, ProbeOutcome::Open),
                // A filtered map port is normal on relay-topology servers
                // (bahamut) and must not fail the verdict.
                mk(PortRole::Map, ProbeOutcome::TimedOut),
            ],
        };
        assert!(report.game_ports_ok());

        let report = ConnectivityReport {
            address: "test".into(),
            probes: vec![
                mk(PortRole::Lobby, ProbeOutcome::TimedOut),
                mk(PortRole::World, ProbeOutcome::Open),
                mk(PortRole::Map, ProbeOutcome::Open),
            ],
        };
        assert!(!report.game_ports_ok());
    }

    #[test]
    fn task_delivers_exactly_one_report() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let _keep = &listener;
        let mut task = ConnectivityTask::spawn("127.0.0.1".to_string(), FAST, true);
        let deadline = Instant::now() + Duration::from_secs(10);
        let report = loop {
            if let Some(r) = task.try_recv() {
                break r;
            }
            assert!(Instant::now() < deadline, "probe never completed");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(report.probes.len(), 3);
        // Exactly once: the channel is drained after the first receive.
        assert!(task.try_recv().is_none());
    }

    #[test]
    fn summary_names_every_port_and_outcome() {
        let report = ConnectivityReport {
            address: "bahamut.example".into(),
            probes: vec![PortProbe {
                role: PortRole::Lobby,
                port: LOBBY_PORT,
                outcome: ProbeOutcome::TimedOut,
                elapsed: Duration::ZERO,
            }],
        };
        let s = report.summary();
        assert!(s.contains("bahamut.example"));
        assert!(s.contains("lobby 54994"));
        assert!(s.contains("timed out"));
    }
}
