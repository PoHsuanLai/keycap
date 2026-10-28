//! Where keycap's virtual devices come from: the root service that owns `/dev/uinput` and hands
//! over a finished device, or `/dev/uinput` itself. The desktop's install has the service and no
//! access to the node; the standalone install has the node and no service.

use super::pointer::PointerCaps;
use super::twin;
use super::uinput::{self, OutputError};
use rustix::fd::OwnedFd;
use std::path::Path;
use turnstile_client::{AbsAxis, PointerCaps as ServiceCaps};
use turnstile_client::{Broker, ClientError, Create, DeviceSource, Made, Session};

/// Where the service listens when nothing overrides it.
pub(crate) const DEFAULT_SERVICE_SOCKET: &str = turnstile_client::DEFAULT_SOCKET;

/// Where a device is made.
#[derive(Debug, Clone)]
pub(crate) enum Source {
    /// Open `/dev/uinput` as this user and set the device up. The standalone install.
    Direct,
    /// Ask the service, which answers with the finished device's descriptor.
    Service(Broker),
    /// A stand-in for the service, for tests.
    #[cfg(test)]
    Introduced(turnstile_client::fake::FakeSource),
}

impl Source {
    /// The service when its socket is at `socket`, else `/dev/uinput`.
    pub(crate) fn detect(socket: &Path) -> Self {
        match socket.exists() {
            true => Source::Service(Broker::at(socket)),
            false => Source::Direct,
        }
    }

    /// Where devices come from, for the log.
    pub(crate) fn describe(&self) -> &'static str {
        match self {
            Source::Direct => "/dev/uinput",
            Source::Service(_) => "the device service",
            #[cfg(test)]
            Source::Introduced(_) => "a stand-in for the device service",
        }
    }

    /// A finished "keycap keyboard".
    pub(crate) fn keyboard(&self) -> Result<OwnedFd, OutputError> {
        self.make(&Create::keyboard(), uinput::direct_keyboard)
    }

    /// A finished pointer with `caps`.
    pub(crate) fn pointer(&self, caps: &PointerCaps) -> Result<OwnedFd, OutputError> {
        self.make(&Create::pointer(service_caps(caps)), || {
            twin::direct_pointer(caps)
        })
    }

    /// `request` to the service, or `direct` when there is none to ask.
    fn make(
        &self,
        request: &Create,
        direct: impl FnOnce() -> Result<OwnedFd, OutputError>,
    ) -> Result<OwnedFd, OutputError> {
        match self {
            Source::Direct => direct(),
            Source::Service(broker) => ask(broker, request, direct),
            #[cfg(test)]
            Source::Introduced(fake) => ask(fake, request, direct),
        }
    }
}

/// Asks `source` for a device. A service that is not there falls back to `direct` with a
/// word in the log; a refusal or any other failure is the answer.
fn ask(
    source: &impl DeviceSource,
    request: &Create,
    direct: impl FnOnce() -> Result<OwnedFd, OutputError>,
) -> Result<OwnedFd, OutputError> {
    match handed(source, request) {
        Ok(fd) => Ok(fd),
        Err(ClientError::Absent) => {
            eprintln!("keycap: the device service is not running; opening the device directly");
            direct()
        }
        Err(e) => Err(OutputError::Unavailable(e.to_string())),
    }
}

/// The descriptor of the device `source` made for `request`. The connection is closed on the
/// way out: a handed device belongs to its holder and outlives the connection.
fn handed(source: &impl DeviceSource, request: &Create) -> Result<OwnedFd, ClientError> {
    match source.open()?.create(request)? {
        Made::Handed { fd, .. } => Ok(fd),
        Made::Proxied(_) => Err(ClientError::Unexpected),
    }
}

/// What the service is asked for a pointer: the original's name as the label (the service adds
/// the twin's suffix), and the same buttons, axes and properties.
fn service_caps(caps: &PointerCaps) -> ServiceCaps {
    ServiceCaps {
        label: caps.label.clone(),
        vendor: caps.vendor,
        product: caps.product,
        props: caps.props.clone(),
        buttons: caps.buttons.clone(),
        rels: caps.rel.clone(),
        abs: caps
            .abs
            .iter()
            .map(|axis| AbsAxis {
                code: axis.code,
                minimum: axis.info.minimum,
                maximum: axis.info.maximum,
                fuzz: axis.info.fuzz,
                flat: axis.info.flat,
                resolution: axis.info.resolution,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::events::{EV_KEY, InputEvent};
    use crate::daemon::pointer::AbsAxis as TwinAxis;
    use crate::daemon::scratch::Scratch;
    use crate::daemon::twin::PointerTwin;
    use crate::daemon::uinput::{Device, Emitter, PointerSink};
    use keycap_sys::AbsInfo;
    use rustix::net::{AddressFamily, RecvFlags, SocketFlags, SocketType, socketpair};
    use turnstile_client::Reason;
    use turnstile_client::fake::{FakeSource, OpenMode};

    const BTN_LEFT: u16 = 0x110;

    fn press(code: u16) -> InputEvent {
        InputEvent {
            kind: EV_KEY,
            code,
            value: 1,
        }
    }

    /// What the holder of the service's end of the `n`th handed descriptor has been sent: one
    /// packet, or `None` when there is none or the descriptor was closed.
    fn arrived(fake: &FakeSource, n: usize) -> Option<Vec<u8>> {
        let state = fake.state();
        let mut buf = [0u8; 64];
        match rustix::net::recv(&state.handed_peers[n], &mut buf, RecvFlags::DONTWAIT) {
            Ok((0, _)) | Err(_) => None,
            Ok((len, _)) => Some(buf[..len].to_vec()),
        }
    }

    fn caps() -> PointerCaps {
        PointerCaps {
            label: "Combo Receiver".to_owned(),
            name: "Combo Receiver (keycap pointer)".to_owned(),
            vendor: 0x05ac,
            product: 0x0250,
            props: vec![0, 2],
            buttons: vec![BTN_LEFT],
            rel: vec![0, 1],
            abs: vec![TwinAxis {
                code: 0,
                info: AbsInfo {
                    value: 0,
                    minimum: -5,
                    maximum: 1200,
                    fuzz: 4,
                    flat: 8,
                    resolution: 12,
                },
            }],
        }
    }

    #[test]
    fn a_keyboard_is_the_descriptor_the_service_hands_over_and_writes_go_through_it() {
        let fake = FakeSource::new();
        let mut device = Device::create(&Source::Introduced(fake.clone())).unwrap();
        {
            let state = fake.state();
            assert_eq!(state.created, [Create::keyboard()]);
            assert_eq!((state.opened, state.handed_peers.len()), (1, 1));
        }
        device.write(&[press(30)]).unwrap();
        assert_eq!(arrived(&fake, 0), Some(press(30).to_bytes().to_vec()));
    }

    #[test]
    fn a_new_keyboard_is_asked_for_and_the_old_descriptor_is_closed() {
        let fake = FakeSource::new();
        let mut device = Device::create(&Source::Introduced(fake.clone())).unwrap();
        device.recreate().unwrap();
        assert_eq!(
            fake.state().created,
            [Create::keyboard(), Create::keyboard()]
        );
        assert_eq!(
            arrived(&fake, 0),
            None,
            "the first device's descriptor is closed"
        );
        device.write(&[press(31)]).unwrap();
        assert_eq!(arrived(&fake, 1), Some(press(31).to_bytes().to_vec()));
    }

    #[test]
    fn a_pointer_is_asked_for_by_the_originals_name_with_its_axes_and_comes_back_as_a_twin() {
        let fake = FakeSource::new();
        let source = Source::Introduced(fake.clone());
        let mut twin = PointerTwin::create(&source, &caps()).unwrap();
        let wanted = ServiceCaps {
            label: "Combo Receiver".to_owned(),
            vendor: 0x05ac,
            product: 0x0250,
            props: vec![0, 2],
            buttons: vec![BTN_LEFT],
            rels: vec![0, 1],
            abs: vec![AbsAxis {
                code: 0,
                minimum: -5,
                maximum: 1200,
                fuzz: 4,
                flat: 8,
                resolution: 12,
            }],
        };
        assert_eq!(fake.state().created, [Create::pointer(wanted)]);
        twin.write(&[press(BTN_LEFT)]).unwrap();
        assert_eq!(arrived(&fake, 0), Some(press(BTN_LEFT).to_bytes().to_vec()));
    }

    #[test]
    fn a_refusal_is_the_answer_and_nothing_is_opened_directly() {
        let fake = FakeSource::new();
        fake.state().refuse_next = Some(Reason::NotAllowed);
        let err = Device::create(&Source::Introduced(fake.clone())).unwrap_err();
        assert!(
            matches!(&err, OutputError::Unavailable(why) if why.contains("refused")),
            "{err}"
        );
        assert_eq!(fake.state().created.len(), 1, "asked once, not again");
    }

    #[test]
    fn a_service_that_is_not_there_falls_back_to_the_given_way_of_making_the_device() {
        let fake = FakeSource::new();
        fake.state().open_mode = OpenMode::Absent;
        let (ours, _theirs) = socketpair(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC,
            None,
        )
        .unwrap();
        let made = ask(&fake, &Create::keyboard(), || Ok(ours));
        assert!(made.is_ok(), "the fallback ran");
        assert!(
            fake.state().created.is_empty(),
            "nothing was asked of the service"
        );
    }

    #[test]
    fn the_service_is_used_when_its_socket_is_there_and_the_node_when_it_is_not() {
        let dir = Scratch::new("source");
        let socket = dir.path().join("input.sock");
        assert!(matches!(Source::detect(&socket), Source::Direct));
        std::fs::write(&socket, "").unwrap();
        assert!(matches!(Source::detect(&socket), Source::Service(_)));
    }
}
