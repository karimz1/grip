//! Native socket interoperability and process-to-path association.
use oflh_core::{Cancellation, Protocol, Target};
use std::{
    fs::File,
    net::{TcpListener, UdpSocket},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Fixture(Child);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn port_fixture() {
    let Some(directory) = std::env::var_os("OFLH_PORT_FIXTURE") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let _file = File::open(directory.join("project.bin")).unwrap();
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let tcp6 = TcpListener::bind("[::1]:0").unwrap();
    let udp6 = UdpSocket::bind("[::1]:0").unwrap();
    std::fs::write(
        directory.join("ready"),
        format!(
            "{} {} {} {}",
            tcp.local_addr().unwrap().port(),
            udp.local_addr().unwrap().port(),
            tcp6.local_addr().unwrap().port(),
            udp6.local_addr().unwrap().port()
        ),
    )
    .unwrap();
    while !directory.join("release").exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
    drop((tcp, udp, tcp6, udp6));
    std::fs::write(directory.join("released"), b"ready").unwrap();
    loop {
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for(path: &std::path::Path, child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "port fixture exited early"
        );
        assert!(Instant::now() < deadline, "port fixture timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn native_tcp_udp_owners_and_release() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("project.bin"), b"fixture").unwrap();
    let mut fixture = Fixture(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "port_fixture", "--nocapture"])
            .env("OFLH_PORT_FIXTURE", directory.path())
            .current_dir(directory.path())
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for(&directory.path().join("ready"), &mut fixture.0);
    let numbers: Vec<u16> = std::fs::read_to_string(directory.path().join("ready"))
        .unwrap()
        .split_whitespace()
        .map(|port| port.parse().unwrap())
        .collect();
    let cancel = Cancellation::default();
    let ports = oflh_platform::scan_ports(&cancel).unwrap();
    let owner = ports
        .processes
        .iter()
        .find(|process| process.identity.pid == fixture.0.id())
        .expect("native socket owner is visible");
    assert_ne!(owner.identity.started, 0);
    for (index, protocol) in [Protocol::Tcp, Protocol::Udp, Protocol::Tcp, Protocol::Udp]
        .into_iter()
        .enumerate()
    {
        assert!(
            owner.ports.iter().any(|port| port.number == numbers[index]
                && port.protocol == protocol
                && port.address.is_ipv6() == (index >= 2)),
            "missing binding {index}"
        );
    }
    let mut backend = oflh_platform::native().unwrap();
    let target = Target::new(directory.path()).unwrap();
    let mut files = backend.scan(&target, &cancel).unwrap();
    files.processes.extend(ports.processes.clone());
    files.normalize();
    let joined = files
        .processes
        .iter()
        .find(|process| process.identity == owner.identity)
        .unwrap();
    assert!(
        !joined.usages.is_empty(),
        "file references survive joining port records"
    );
    assert!(joined.ports.len() >= 4);
    std::fs::write(directory.path().join("release"), b"release").unwrap();
    wait_for(&directory.path().join("released"), &mut fixture.0);
    let refreshed = oflh_platform::scan_ports(&cancel).unwrap();
    assert!(
        !refreshed
            .processes
            .iter()
            .any(|process| process.identity == owner.identity)
    );
    cancel.cancel();
    assert!(matches!(
        oflh_platform::scan_ports(&cancel),
        Err(oflh_core::Error::Cancelled)
    ));
}
