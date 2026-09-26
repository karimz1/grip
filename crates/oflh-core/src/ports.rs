//! Local network bindings and exact-port search semantics.
use crate::{
    Process,
    search::{Field, Query, Scratch},
};
use std::net::IpAddr;

/// Transport protocol of a local binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Protocol {
    /// A TCP socket in the listening state.
    Tcp,
    /// A bound UDP socket; UDP has no listening handshake.
    Udp,
}
impl Protocol {
    /// Stable display and search label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Tcp => "TCP",
            Self::Udp => "UDP",
        }
    }
    /// Describe the evidence without claiming external reachability.
    pub fn state(self) -> &'static str {
        match self {
            Self::Tcp => "LISTEN",
            Self::Udp => "BOUND",
        }
    }
}

/// An observed local binding, not proof of network reachability.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Port {
    /// TCP listener or bound UDP socket.
    pub protocol: Protocol,
    /// Local interface address; an unspecified address means all interfaces.
    pub address: IpAddr,
    /// Local port number in host byte order.
    pub number: u16,
}

/// Cached fields for a process-owned binding.
pub struct PortIndex {
    fields: Vec<Field>,
}
impl PortIndex {
    /// Build textual fields once per snapshot, preserving numeric-port matching separately.
    pub fn new(process: &Process, port: &Port) -> Self {
        Self {
            fields: vec![
                Field::new(&process.name, true),
                Field::new(&process.identity.pid.to_string(), false),
                Field::new(&format!("pid:{}", process.identity.pid), false),
                Field::new(&process.user, false),
                Field::new(&process.executable.to_string_lossy(), false),
                Field::new(&process.cwd.to_string_lossy(), false),
                Field::new(port.protocol.label(), false),
                Field::new(port.protocol.state(), false),
                Field::new(&port.address.to_string(), false),
                Field::new(
                    if port.address.is_ipv4() {
                        "ipv4"
                    } else {
                        "ipv6"
                    },
                    false,
                ),
            ],
        }
    }
}

/// Text search with exact numeric port terms (for example `3000` or `port:3000`).
pub struct PortQuery {
    ports: Vec<u16>,
    text: Query,
    valid: bool,
}
impl PortQuery {
    /// Compile all terms as an AND query. Invalid port numbers match nothing.
    pub fn new(text: &str) -> Self {
        let mut ports = Vec::new();
        let mut words = Vec::new();
        let mut valid = true;
        for term in text.split_whitespace() {
            let lower = term.to_ascii_lowercase();
            let number = lower.strip_prefix("port:").or_else(|| {
                term.bytes()
                    .all(|byte| byte.is_ascii_digit())
                    .then_some(term)
            });
            if let Some(number) = number {
                match number.parse::<u16>() {
                    Ok(number) if number > 0 => ports.push(number),
                    _ => valid = false,
                }
            } else {
                words.push(term);
            }
        }
        Self {
            ports,
            text: Query::new(&words.join(" ")),
            valid,
        }
    }
    /// Match a binding and its cached owner metadata.
    pub fn matches(&self, port: &Port, index: &PortIndex, scratch: &mut Scratch) -> bool {
        self.valid
            && self.ports.iter().all(|number| *number == port.number)
            && self.text.matches(&index.fields, scratch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_terms_are_exact_ports_not_pid_or_substrings() {
        let process = Process {
            name: "server".into(),
            ..Process::default()
        };
        let port = Port {
            protocol: Protocol::Tcp,
            address: "127.0.0.1".parse().unwrap(),
            number: 8080,
        };
        let index = PortIndex::new(&process, &port);
        for (query, expected) in [
            ("8080", true),
            ("80", false),
            ("port:8080 server tcp", true),
            ("8080 udp", false),
            ("65536", false),
            ("port:bad", false),
            ("", true),
            ("pid:0", true),
        ] {
            assert_eq!(
                PortQuery::new(query).matches(&port, &index, &mut Scratch::default()),
                expected,
                "{query}"
            );
        }
    }
}
