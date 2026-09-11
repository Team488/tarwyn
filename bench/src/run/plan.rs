//! Which server answers a case, which probe touches it, and how each is run.
//!
//! This is the whole launcher table. It lives beside [`crate::catalog`] and in
//! the same language, because a table that says how to run a case and a
//! catalog that says which cases exist have to agree, and two files in two
//! languages do not.

use super::Settings;
use super::env::Env;
use std::time::Duration;

/// Which server answers a case.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Server {
    Rust,
    Ntcore,
    Tarwyn,
}

/// Which probe touches it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Probe {
    Rust,
    Python,
    JavaClient,
    JavaSocket,
}

impl Probe {
    /// Whether this probe sends on the thread that paced the send.
    ///
    /// Only a probe that does may be pinned to one core. A JVM probe runs its
    /// transport on threads of its own, one for JeroMQ and a pool for
    /// `TarwynClient`, and pinning starves them against the pacer's spin.
    pub(crate) fn pinnable(self) -> bool {
        !matches!(self, Probe::JavaClient | Probe::JavaSocket)
    }
}

/// Everything that differs between implementations, in one table.
///
/// Adding an implementation is a line here and a line in [`crate::catalog`].
pub(crate) struct Plan {
    pub(crate) server: Server,
    pub(crate) probe: Probe,
    pub(crate) port: u16,
    pub(crate) settle: Duration,
}

pub(crate) fn plan(case: &str, implementation: &str) -> Plan {
    let jvm_settle = Duration::from_secs(8);
    match (case, implementation) {
        ("publish", "ntcore") => Plan {
            server: Server::Ntcore,
            probe: Probe::Rust,
            port: 48820,
            settle: Duration::ZERO,
        },
        ("publish", "tarwyn") => Plan {
            server: Server::Tarwyn,
            probe: Probe::JavaSocket,
            port: 48800,
            settle: jvm_settle,
        },
        ("publish_client", "ntcore") => Plan {
            server: Server::Ntcore,
            probe: Probe::Python,
            port: 48820,
            settle: Duration::ZERO,
        },
        ("publish_client", "tarwyn") => Plan {
            server: Server::Tarwyn,
            probe: Probe::JavaClient,
            port: 48800,
            settle: jvm_settle,
        },
        _ => Plan {
            server: Server::Rust,
            probe: Probe::Rust,
            port: 5810,
            settle: Duration::ZERO,
        },
    }
}

/// The command that starts a server, given the port it should listen on.
///
/// TARWYN is started with `java -jar`, the invocation its release notes give;
/// the jar's manifest names the same main class `-cp` would.
pub(crate) fn server_command(
    env: &Env,
    server: Server,
    port: u16,
) -> Option<(String, Vec<String>)> {
    match server {
        Server::Rust => Some((env.server.display().to_string(), Vec::new())),
        Server::Ntcore => Some((
            "uv".to_string(),
            vec![
                "run".into(),
                "--quiet".into(),
                "--with".into(),
                env.pyntcore.clone(),
                "python".into(),
                env.python_probe().display().to_string(),
                "server".into(),
                "--port".into(),
                port.to_string(),
            ],
        )),
        Server::Tarwyn => Some((
            "java".to_string(),
            vec!["-jar".into(), env.tarwyn_jar.clone()?],
        )),
    }
}

/// The command that runs one side of a case with one probe.
///
/// Only a server on a non-default port is named on the command line; the
/// probes know their own defaults.
#[expect(clippy::too_many_arguments)]
pub(crate) fn probe_command(
    env: &Env,
    settings: &Settings,
    probe: Probe,
    role: &str,
    case: &str,
    implementation: &str,
    payload: usize,
    port: u16,
    server: Server,
) -> Option<(String, Vec<String>)> {
    let subscriber = role == "subscriber";
    match probe {
        Probe::Rust => {
            let mut args = vec![
                "run".to_string(),
                "--case".into(),
                case.into(),
                "--impl".into(),
                implementation.into(),
                "--role".into(),
                role.into(),
                "--payload".into(),
                payload.to_string(),
                "--version".into(),
                env.version_of(implementation),
            ];
            if server == Server::Ntcore {
                args.push("--host".into());
                args.push(format!("127.0.0.1:{port}"));
            }
            if subscriber {
                args.push("--samples".into());
                args.push(settings.samples.to_string());
            } else {
                args.push("--rate".into());
                args.push(settings.rate_hz.to_string());
                args.push("--count".into());
                args.push(settings.count.to_string());
            }
            Some((env.exe.display().to_string(), args))
        }
        Probe::Python => {
            let mut args = vec![
                "run".to_string(),
                "--quiet".into(),
                "--with".into(),
                env.pyntcore.clone(),
                "python".into(),
                env.python_probe().display().to_string(),
                role.into(),
                "--port".into(),
                port.to_string(),
                "--payload".into(),
                payload.to_string(),
            ];
            if subscriber {
                args.push("--samples".into());
                args.push(settings.total_samples().to_string());
            } else {
                args.push("--rate".into());
                args.push(settings.rate_hz.to_string());
                args.push("--count".into());
                args.push(settings.count.to_string());
            }
            Some(("uv".to_string(), args))
        }
        Probe::JavaClient | Probe::JavaSocket => {
            let which = if probe == Probe::JavaSocket {
                "socket"
            } else {
                "client"
            };
            let mut args = vec![
                "-cp".to_string(),
                env.java_classpath.clone()?,
                "tarwyn.Main".into(),
                role.into(),
                "--probe".into(),
                which.into(),
                "--payload".into(),
                payload.to_string(),
            ];
            if subscriber {
                args.push("--samples".into());
                args.push(settings.total_samples().to_string());
            } else {
                args.push("--rate".into());
                args.push(settings.rate_hz.to_string());
                args.push("--count".into());
                args.push(settings.count.to_string());
            }
            Some(("java".to_string(), args))
        }
    }
}

/// Run one case for one implementation at one payload, returning its `ROW`.
#[cfg(test)]
mod tests {
    use super::{Probe, plan};

    #[test]
    fn every_cataloged_pairing_has_a_plan() {
        for case in crate::catalog::CASES {
            for implementation in case.implementations {
                let plan = plan(case.name, implementation);
                assert!(plan.port > 0, "{}/{implementation} has no port", case.name);
            }
        }
    }

    #[test]
    fn only_a_probe_that_sends_on_its_own_thread_is_pinnable() {
        assert!(Probe::Rust.pinnable());
        assert!(Probe::Python.pinnable());
        assert!(!Probe::JavaClient.pinnable());
        assert!(!Probe::JavaSocket.pinnable());
    }
}
