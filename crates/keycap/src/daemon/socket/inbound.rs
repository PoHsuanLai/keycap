//! Reading what one client sent, a bounded number of packets at a time.

use keycap_protocol::MAX_PACKET;
use rustix::fd::AsFd;
use rustix::io::Errno;
use rustix::net::{RecvFlags, recv};

/// Where a client's stream stands after a read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stream {
    /// The budget ran out with packets possibly still queued: `poll` reports it again.
    More,
    /// Nothing more queued.
    Idle,
    /// The client closed, or reading failed.
    Closed,
}

/// The packets read and how the stream stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Pulled {
    pub packets: Vec<Vec<u8>>,
    pub stream: Stream,
}

/// Reads at most `budget` packets from the nonblocking `fd`. A packet longer than
/// `MAX_PACKET` comes back one byte over the limit, which decoding rejects.
pub(super) fn pull(fd: impl AsFd, budget: usize) -> Pulled {
    let mut buf = vec![0u8; MAX_PACKET + 1];
    let mut packets = Vec::new();
    while packets.len() < budget {
        match recv(&fd, &mut buf, RecvFlags::DONTWAIT) {
            Ok((0, _)) => {
                return Pulled {
                    packets,
                    stream: Stream::Closed,
                };
            }
            Ok((n, _)) => packets.push(buf[..n].to_vec()),
            Err(Errno::AGAIN) => {
                return Pulled {
                    packets,
                    stream: Stream::Idle,
                };
            }
            Err(Errno::INTR) => {}
            Err(_) => {
                return Pulled {
                    packets,
                    stream: Stream::Closed,
                };
            }
        }
    }
    Pulled {
        packets,
        stream: Stream::More,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustix::fd::OwnedFd;
    use rustix::net::{AddressFamily, SendFlags, SocketFlags, SocketType, send, socketpair};

    fn pair() -> (OwnedFd, OwnedFd) {
        socketpair(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
            None,
        )
        .unwrap()
    }

    fn send_n(fd: &OwnedFd, n: usize) {
        for i in 0..n {
            send(fd, format!("m{i}").as_bytes(), SendFlags::NOSIGNAL).unwrap();
        }
    }

    #[test]
    fn a_flood_is_read_a_budget_at_a_time_and_nothing_is_lost() {
        let (ours, theirs) = pair();
        send_n(&theirs, 40);
        let first = pull(&ours, 16);
        assert_eq!((first.packets.len(), first.stream), (16, Stream::More));
        assert_eq!(first.packets[0], b"m0");
        let second = pull(&ours, 16);
        assert_eq!((second.packets.len(), second.stream), (16, Stream::More));
        assert_eq!(second.packets[0], b"m16", "the next pass resumes in order");
        let third = pull(&ours, 16);
        assert_eq!((third.packets.len(), third.stream), (8, Stream::Idle));
        assert_eq!(third.packets[7], b"m39");
    }

    #[test]
    fn exactly_a_budget_of_packets_asks_for_another_look_and_finds_it_idle() {
        let (ours, theirs) = pair();
        send_n(&theirs, 4);
        assert_eq!(pull(&ours, 4).stream, Stream::More);
        let again = pull(&ours, 4);
        assert_eq!((again.packets.len(), again.stream), (0, Stream::Idle));
    }

    #[test]
    fn a_closed_client_is_reported_after_its_last_packets() {
        let (ours, theirs) = pair();
        send_n(&theirs, 2);
        drop(theirs);
        let pulled = pull(&ours, 16);
        assert_eq!((pulled.packets.len(), pulled.stream), (2, Stream::Closed));
    }

    #[test]
    fn an_oversized_packet_arrives_over_the_limit_for_decoding_to_reject() {
        let (ours, theirs) = pair();
        send(&theirs, &vec![b' '; MAX_PACKET + 1], SendFlags::NOSIGNAL).unwrap();
        let pulled = pull(&ours, 16);
        assert_eq!(pulled.packets[0].len(), MAX_PACKET + 1);
    }
}
