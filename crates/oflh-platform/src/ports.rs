//! Socket collection stays on the native worker and never probes remote hosts.
use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState};
use oflh_core::*;
use std::collections::{BTreeMap, HashMap};

#[cfg(target_os = "linux")]
use crate::linux as native;
#[cfg(target_os = "macos")]
use crate::macos as native;
#[cfg(windows)]
use crate::windows as native;

fn socket_error(error: netstat2::error::Error) -> Error {
    use netstat2::error::Error as SocketError;
    let source = match error {
        SocketError::OsError(source)
        | SocketError::FailedToListProcesses(source)
        | SocketError::FailedToQueryFileDescriptors(source) => source,
        SocketError::FailedToGetTcpTable(code) | SocketError::FailedToGetUdpTable(code) => {
            std::io::Error::from_raw_os_error(code)
        }
        error => std::io::Error::other(error),
    };
    io("inspect local ports", source)
}

/// Capture identities before socket enumeration, then revalidate afterward.
/// Never attach an old PID's binding to a new process lifetime. Unattributed
/// bindings use a zero identity and cannot be termination targets.
pub(crate) fn scan(cancel: &Cancellation) -> Result<Snapshot> {
    let before: HashMap<_, _> = native::port_processes(cancel)?
        .into_iter()
        .map(|process| (process.identity.pid, process))
        .collect();
    cancel.check()?;
    let mut snapshot = Snapshot::default();
    let mut bindings: BTreeMap<u32, Vec<Port>> = BTreeMap::new();
    let mut unattributed = Vec::new();
    // Enumerate families independently: unavailable IPv6 must not hide IPv4 results.
    for family in [AddressFamilyFlags::IPV4, AddressFamilyFlags::IPV6] {
        for protocol in [ProtocolFlags::TCP, ProtocolFlags::UDP] {
            cancel.check()?;
            let sockets = match netstat2::iterate_sockets_info(family, protocol) {
                Ok(sockets) => sockets,
                Err(error) => {
                    snapshot.warnings.push(socket_error(error).to_string());
                    continue;
                }
            };
            let mut partial = false;
            for socket in sockets {
                cancel.check()?;
                let socket = match socket {
                    Ok(socket) => socket,
                    Err(error) => {
                        if !partial {
                            snapshot.warnings.push(socket_error(error).to_string());
                        }
                        partial = true;
                        continue;
                    }
                };
                let Some(port) = local_binding(socket.protocol_socket_info) else {
                    continue;
                };
                if socket.associated_pids.is_empty() {
                    unattributed.push(port);
                } else {
                    for pid in socket.associated_pids {
                        bindings.entry(pid).or_default().push(port.clone());
                    }
                }
            }
            if partial {
                snapshot.warnings.push(
                    "Some port entries could not be inspected (permissions or socket changes)."
                        .into(),
                );
            }
        }
    }
    for (pid, ports) in bindings {
        cancel.check()?;
        match verified_owner(&before, pid, native::port_identity) {
            Some(mut process) => {
                process.ports = ports;
                snapshot.processes.push(process);
            }
            _ => unattributed.extend(ports),
        }
    }
    if !unattributed.is_empty() {
        snapshot.warnings.push("Some port owners are unavailable or changed during scanning; their ports are shown without an actionable PID.".into());
        snapshot.processes.push(Process {
            name: "owner unavailable".into(),
            user: "unknown".into(),
            ports: unattributed,
            ..Process::default()
        });
    }
    snapshot.warnings.push("Ports are local TCP listeners and bound UDP sockets, not proof of remote reachability. Permissions and network namespaces limit visibility; container port forwarding is not enumerated.".into());
    snapshot.normalize();
    Ok(snapshot)
}

fn local_binding(socket: ProtocolSocketInfo) -> Option<Port> {
    match socket {
        ProtocolSocketInfo::Tcp(tcp) if tcp.state == TcpState::Listen && tcp.local_port != 0 => {
            Some(Port {
                protocol: Protocol::Tcp,
                address: tcp.local_addr,
                number: tcp.local_port,
            })
        }
        ProtocolSocketInfo::Udp(udp) if udp.local_port != 0 => Some(Port {
            protocol: Protocol::Udp,
            address: udp.local_addr,
            number: udp.local_port,
        }),
        _ => None,
    }
}

fn verified_owner(
    before: &HashMap<u32, Process>,
    pid: u32,
    identity: impl FnOnce(u32) -> Result<Identity>,
) -> Option<Process> {
    let captured = before.get(&pid)?;
    if identity(pid).ok()? != captured.identity {
        return None;
    }
    let mut process = captured.clone();
    let mut parent = process.parent;
    while parent > 0
        && parent != pid
        && process.ancestors.len() < 8
        && !process
            .ancestors
            .iter()
            .any(|ancestor| ancestor.identity.pid == parent)
    {
        let Some(ancestor) = before.get(&parent) else {
            break;
        };
        process.ancestors.push(Ancestor {
            identity: ancestor.identity,
            name: ancestor.name.clone(),
        });
        parent = ancestor.parent;
    }
    Some(process)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_or_unavailable_birth_never_gets_socket_ownership() {
        let identity = Identity {
            pid: 424242,
            started: 10,
            started_sub: 0,
        };
        let before = HashMap::from([(
            identity.pid,
            Process {
                identity,
                ..Process::default()
            },
        )]);
        assert!(verified_owner(&before, identity.pid, |_| Ok(identity)).is_some());
        assert!(
            verified_owner(&before, identity.pid, |_| Ok(Identity {
                started: 11,
                ..identity
            }))
            .is_none()
        );
        assert!(verified_owner(&before, identity.pid, |_| Err(Error::Changed)).is_none());
        assert!(verified_owner(&before, 999, |_| Ok(identity)).is_none());
    }
    #[test]
    fn only_listeners_and_bound_udp_are_ports() {
        let mut tcp = netstat2::TcpSocketInfo {
            local_addr: "127.0.0.1".parse().unwrap(),
            local_port: 3000,
            remote_addr: "127.0.0.1".parse().unwrap(),
            remote_port: 50000,
            state: TcpState::Established,
        };
        assert!(local_binding(ProtocolSocketInfo::Tcp(tcp.clone())).is_none());
        tcp.state = TcpState::Listen;
        assert_eq!(
            local_binding(ProtocolSocketInfo::Tcp(tcp)).unwrap().number,
            3000
        );
        assert!(
            local_binding(ProtocolSocketInfo::Udp(netstat2::UdpSocketInfo {
                local_addr: "0.0.0.0".parse().unwrap(),
                local_port: 0,
            }))
            .is_none()
        );
    }
}
