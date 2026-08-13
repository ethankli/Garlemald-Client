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
    /// The resolved address that produced `outcome` — the one that
    /// answered when open, or the one whose error was reported. `None`
    /// when resolution itself failed. Disambiguates IPv4 from IPv6 in
    /// the log line when a host publishes both.
    pub addr: Option<SocketAddr>,
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
    ///
    /// Both required roles must be *present* and open — a bare `all()`
    /// over the list would return true for a report with no probes at
    /// all (vacuous truth), turning a probe that never ran into a
    /// confident "server reachable".
    pub fn game_ports_ok(&self) -> bool {
        [PortRole::Lobby, PortRole::World].iter().all(|role| {
            self.probes
                .iter()
                .any(|p| p.role == *role && p.outcome.is_open())
        })
    }

    /// One-line human summary, for logs and the GUI status row. The
    /// answering address is included when it differs from the host the
    /// user typed, so a v4/v6 split is visible in a pasted log.
    pub fn summary(&self) -> String {
        if self.probes.is_empty() {
            return format!("{} — probe did not run", self.address);
        }
        let parts: Vec<String> = self
            .probes
            .iter()
            .map(|p| match p.addr {
                Some(addr) => format!("{} {} [{}]: {}", p.role, p.port, addr.ip(), p.outcome),
                None => format!("{} {}: {}", p.role, p.port, p.outcome),
            })
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

/// How informative an outcome is when several addresses disagree, worst
/// (0) to best. `Refused` outranks `TimedOut` because it is *definitive*
/// — the host answered, so the network path is fine and only the service
/// is missing — where a timeout cannot distinguish a firewall from a
/// down host.
fn outcome_rank(outcome: &ProbeOutcome) -> u8 {
    match outcome {
        ProbeOutcome::Open => 4,
        ProbeOutcome::Refused => 3,
        ProbeOutcome::Error(_) => 2,
        ProbeOutcome::TimedOut => 1,
        ProbeOutcome::ResolveFailed(_) => 0,
    }
}

/// Race every resolved address for one port and report the best answer.
///
/// A hostname can resolve to several addresses (round-robin A records, or
/// an AAAA the local box has no route to). Probing only the first would
/// report a reachable server as unreachable whenever the resolver happens
/// to order a dead address first — the game itself walks the whole list,
/// so the diagnostic must too, or it manufactures exactly the false
/// verdict it exists to prevent.
///
/// The addresses are raced concurrently rather than tried in sequence so
/// the worst-case wall time stays one `timeout` regardless of how many
/// addresses the host publishes.
fn probe_addrs(addrs: &[SocketAddr], timeout: Duration) -> (ProbeOutcome, Option<SocketAddr>) {
    if addrs.is_empty() {
        return (
            ProbeOutcome::ResolveFailed("no addresses returned".to_string()),
            None,
        );
    }
    let mut results: Vec<(ProbeOutcome, SocketAddr)> = std::thread::scope(|scope| {
        let handles: Vec<_> = addrs
            .iter()
            .map(|&addr| {
                scope.spawn(move || {
                    let outcome = match TcpStream::connect_timeout(&addr, timeout) {
                        Ok(_stream) => ProbeOutcome::Open,
                        Err(e) => classify_error(&e),
                    };
                    (outcome, addr)
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().ok())
            .collect::<Vec<_>>()
    });
    // Every join failed (all probe threads panicked) — report it rather
    // than silently claiming the host resolved to nothing.
    if results.is_empty() {
        return (
            ProbeOutcome::Error("probe threads panicked".to_string()),
            None,
        );
    }
    results.sort_by_key(|(outcome, _)| std::cmp::Reverse(outcome_rank(outcome)));
    let (outcome, addr) = results.remove(0);
    (outcome, Some(addr))
}

/// Probe one `host:port` with a bounded connect across every address the
/// host resolves to.
fn probe_port(host: &str, role: PortRole, port: u16, timeout: Duration) -> PortProbe {
    let started = Instant::now();
    let addrs: Vec<SocketAddr> = match (host, port).to_socket_addrs() {
        Ok(it) => it.collect(),
        Err(e) => {
            return PortProbe {
                role,
                port,
                outcome: ProbeOutcome::ResolveFailed(e.to_string()),
                addr: None,
                elapsed: started.elapsed(),
            };
        }
    };
    let (outcome, addr) = probe_addrs(&addrs, timeout);
    PortProbe {
        role,
        port,
        outcome,
        addr,
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
            .map(|&(role, port)| {
                (
                    role,
                    port,
                    scope.spawn(move || probe_port(address, role, port, timeout)),
                )
            })
            .collect();
        handles
            .into_iter()
            .map(|(role, port, h)| {
                // Keep the role/port of the probe that actually died: a
                // fallback that always claimed `Lobby` would turn a
                // panicked *map* probe into a false lobby failure, and
                // lobby failures are what block the launch verdict.
                h.join().unwrap_or_else(|_| PortProbe {
                    role,
                    port,
                    outcome: ProbeOutcome::Error("probe thread panicked".to_string()),
                    addr: None,
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
    /// The address being probed, so a caller can tell whether a landed
    /// report still describes the server currently selected.
    address: String,
    /// Set once a report has been handed out. The worker drops its
    /// sender as it exits, so without this latch the poll *after* a
    /// successful receive would see `Disconnected` and synthesise a
    /// second, failed report — overwriting the real one.
    delivered: bool,
    /// Whether the GUI should surface a success as an info message
    /// (a user-initiated check) or stay quiet about it (an automatic
    /// pre-launch probe, where only failures matter).
    pub announce_success: bool,
}

impl ConnectivityTask {
    pub fn spawn(address: String, timeout: Duration, announce_success: bool) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let probe_address = address.clone();
        // A failed spawn must not take the GUI down with it: the dropped
        // sender disconnects the channel, which `try_recv` reports as a
        // failed probe on the next poll.
        if let Err(e) = std::thread::Builder::new()
            .name("connectivity-probe".to_string())
            .spawn(move || {
                // A dropped receiver (task discarded mid-probe) is fine.
                let _ = tx.send(probe_server(&probe_address, timeout));
            })
        {
            log::warn!("could not spawn the connectivity probe thread: {e}");
        }
        Self {
            rx,
            address,
            delivered: false,
            announce_success,
        }
    }

    /// Non-blocking: `Some(report)` exactly once, when the probe finishes.
    ///
    /// A worker that ended without sending (only reachable if the OS
    /// refused to spawn a thread) disconnects the channel; that is
    /// surfaced as a synthetic failed report rather than folded into the
    /// `Empty` case, because a caller that polls until `Some` would
    /// otherwise wait forever — wedging its "in flight" flag and any
    /// repaint loop keyed to it.
    pub fn try_recv(&mut self) -> Option<ConnectivityReport> {
        if self.delivered {
            return None;
        }
        let report = match self.rx.try_recv() {
            Ok(report) => report,
            Err(std::sync::mpsc::TryRecvError::Empty) => return None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => ConnectivityReport {
                address: self.address.clone(),
                probes: Vec::new(),
            },
        };
        self.delivered = true;
        Some(report)
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

    /// A hostname resolving to a dead address *ahead* of a live one is
    /// the regression that matters: probing only the first would call a
    /// reachable server unreachable. Driving `probe_addrs` directly
    /// makes it deterministic without depending on the host resolver.
    #[test]
    fn a_dead_address_ahead_of_a_live_one_still_reports_open() {
        let live = TcpListener::bind("127.0.0.1:0").expect("bind live listener");
        let live_addr = live.local_addr().unwrap();

        // Bind-then-drop: a real address that refuses immediately.
        let dead = TcpListener::bind("127.0.0.1:0").expect("bind dead listener");
        let dead_addr = dead.local_addr().unwrap();
        drop(dead);

        let (outcome, answered) = probe_addrs(&[dead_addr, live_addr], FAST);
        assert_eq!(outcome, ProbeOutcome::Open);
        assert_eq!(answered, Some(live_addr), "the live address should answer");
    }

    #[test]
    fn all_addresses_dead_reports_the_most_definitive_outcome() {
        let dead = TcpListener::bind("127.0.0.1:0").expect("bind dead listener");
        let dead_addr = dead.local_addr().unwrap();
        drop(dead);
        // 203.0.113.0/24 is TEST-NET-3 (RFC 5737) — reserved for
        // documentation and never routed, so it cannot answer.
        let blackhole: SocketAddr = "203.0.113.1:54994".parse().unwrap();

        let (outcome, _) = probe_addrs(&[blackhole, dead_addr], Duration::from_millis(600));
        // Refused outranks TimedOut: it proves the host is up, which is
        // the more actionable of the two for an operator.
        assert_eq!(outcome, ProbeOutcome::Refused);
    }

    #[test]
    fn no_addresses_reports_resolve_failed() {
        let (outcome, addr) = probe_addrs(&[], FAST);
        assert!(matches!(outcome, ProbeOutcome::ResolveFailed(_)));
        assert!(addr.is_none());
    }

    #[test]
    fn empty_report_is_not_treated_as_reachable() {
        // A probe that never ran must not read as "server reachable" —
        // `all()` over an empty list is vacuously true, so the verdict
        // has to require both roles to be present.
        let report = ConnectivityReport {
            address: "test".into(),
            probes: Vec::new(),
        };
        assert!(!report.game_ports_ok());
    }

    #[test]
    fn verdict_requires_lobby_and_world_but_not_map() {
        let mk = |role, outcome| PortProbe {
            role,
            port: 0,
            outcome,
            addr: None,
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
        // Probes the real fixed game ports on loopback; whatever they
        // answer, the task must deliver exactly one three-probe report.
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
                addr: Some("192.0.2.7:54994".parse().unwrap()),
                elapsed: Duration::ZERO,
            }],
        };
        let s = report.summary();
        assert!(s.contains("bahamut.example"));
        assert!(s.contains("lobby 54994"));
        assert!(s.contains("timed out"));
        // The answering address is what disambiguates a v4/v6 split in a
        // pasted log, so it must survive into the summary line.
        assert!(s.contains("192.0.2.7"));
    }
}
