//! Bounded network transport for the developer-preview server.
use carbon_protocol::{decode_varint, PacketError, VarIntError, MAX_PACKET_SIZE};
use std::{
    collections::HashMap,
    io,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    time::{timeout_at, Instant},
};

pub(super) const MAX_CONNECTIONS: usize = 128;
pub(super) const MAX_CONNECTIONS_PER_IP: usize = 8;
const FRAME_TIMEOUT: Duration = Duration::from_secs(30);
const SETUP_TIMEOUT: Duration = Duration::from_secs(30);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) struct Budget {
    available: f64,
    capacity: f64,
    per_second: f64,
    updated: Instant,
}
impl Budget {
    pub(super) fn new(capacity: usize, per_second: usize) -> Self {
        Self {
            available: capacity as f64,
            capacity: capacity as f64,
            per_second: per_second as f64,
            updated: Instant::now(),
        }
    }
    pub(super) fn take(&mut self, amount: usize) -> bool {
        let now = Instant::now();
        self.available = (self.available
            + now.duration_since(self.updated).as_secs_f64() * self.per_second)
            .min(self.capacity);
        self.updated = now;
        if amount as f64 > self.available {
            return false;
        }
        self.available -= amount as f64;
        true
    }
}

#[derive(Default)]
struct Counts {
    total: usize,
    ips: HashMap<IpAddr, usize>,
}
#[derive(Clone, Default)]
pub(super) struct Admissions(Arc<Mutex<Counts>>);
pub(super) struct Admission {
    counts: Admissions,
    ip: IpAddr,
}
impl Admissions {
    pub(super) fn acquire(&self, ip: IpAddr) -> Option<Admission> {
        // Treat IPv4 and IPv4-mapped IPv6 as the same source.
        let ip = match ip {
            IpAddr::V6(ip) => ip.to_ipv4_mapped().map_or(IpAddr::V6(ip), IpAddr::V4),
            ip => ip,
        };
        let mut counts = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if counts.total >= MAX_CONNECTIONS
            || counts.ips.get(&ip).copied().unwrap_or(0) >= MAX_CONNECTIONS_PER_IP
        {
            return None;
        }
        counts.total += 1;
        *counts.ips.entry(ip).or_default() += 1;
        Some(Admission {
            counts: self.clone(),
            ip,
        })
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        let mut counts = self
            .counts
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        counts.total -= 1;
        if let Some(count) = counts.ips.get_mut(&self.ip) {
            *count -= 1;
            if *count == 0 {
                counts.ips.remove(&self.ip);
            }
        }
    }
}

pub(super) struct Connection<S = TcpStream> {
    stream: S,
    prefix: Vec<u8>,
    payload: Vec<u8>,
    received: usize,
    frame_deadline: Option<Instant>,
    setup_deadline: Option<Instant>,
    shutdown: Option<tokio::sync::watch::Receiver<bool>>,
    frames: Budget,
    bytes: Budget,
}
impl<S: AsyncRead + AsyncWrite + Unpin> Connection<S> {
    pub(super) fn new(stream: S) -> Self {
        Self {
            stream,
            prefix: Vec::with_capacity(5),
            payload: Vec::new(),
            received: 0,
            frame_deadline: None,
            shutdown: None,
            setup_deadline: Some(Instant::now() + SETUP_TIMEOUT),
            frames: Budget::new(240, 120),
            bytes: Budget::new(MAX_PACKET_SIZE + 5, 1024 * 1024),
        }
    }
    pub(super) fn observe_shutdown(&mut self, shutdown: tokio::sync::watch::Receiver<bool>) {
        self.shutdown = Some(shutdown);
    }
    pub(super) fn enter_play(&mut self) {
        self.setup_deadline = None;
    }
    fn deadline(&self, normal: Instant) -> Instant {
        self.setup_deadline
            .map_or(normal, |setup| setup.min(normal))
    }
    pub(super) async fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        let deadline = self.deadline(Instant::now() + WRITE_TIMEOUT);
        tokio::select! {
            biased;
            _ = stopped(&mut self.shutdown) => Err(io::Error::new(io::ErrorKind::Interrupted, "server stopping")),
            result = timeout_at(deadline, self.stream.write_all(bytes)) => result.map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "network write deadline exceeded"))?,
        }
    }
    pub(super) async fn shutdown(&mut self) -> io::Result<()> {
        let deadline = self.deadline(Instant::now() + WRITE_TIMEOUT);
        timeout_at(deadline, self.stream.shutdown())
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "network shutdown deadline exceeded",
                )
            })?
    }
    // State lives on the connection, not the future: cancelling a select branch
    // cannot discard a partially consumed prefix/body or restart its deadline.
    pub(super) async fn read_frame(&mut self) -> Result<Vec<u8>, PacketError> {
        let normal = *self
            .frame_deadline
            .get_or_insert_with(|| Instant::now() + FRAME_TIMEOUT);
        let deadline = self.deadline(normal);
        loop {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "network frame deadline exceeded",
                )
                .into());
            }
            if self.payload.is_empty() {
                let mut byte = [0];
                let count =
                    receive(&mut self.stream, &mut self.shutdown, deadline, &mut byte).await?;
                if count == 0 {
                    return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
                }
                if !self.bytes.take(count) {
                    return Err(io::Error::other("inbound byte rate exceeded").into());
                }
                self.prefix.push(byte[0]);
                if byte[0] & 0x80 != 0 {
                    if self.prefix.len() == 5 {
                        return Err(VarIntError::TooLarge.into());
                    }
                    continue;
                }
                // A signed i32 VarInt has only four usable bits in byte five.
                if self.prefix.len() == 5 && byte[0] & 0xf0 != 0 {
                    return Err(VarIntError::TooLarge.into());
                }
                let length = decode_varint(&self.prefix)?.0;
                if length <= 0 {
                    return Err(PacketError::InvalidLength(length));
                }
                if length as usize > MAX_PACKET_SIZE {
                    return Err(PacketError::TooLarge {
                        actual: length as usize,
                        limit: MAX_PACKET_SIZE,
                    });
                }
                self.payload.resize(length as usize, 0);
            }
            let count = receive(
                &mut self.stream,
                &mut self.shutdown,
                deadline,
                &mut self.payload[self.received..],
            )
            .await?;
            if count == 0 {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
            }
            self.received += count;
            if !self.bytes.take(count) {
                return Err(io::Error::other("inbound byte rate exceeded").into());
            }
            if self.received == self.payload.len() {
                if !self.frames.take(1) {
                    return Err(io::Error::other("inbound packet rate exceeded").into());
                }
                self.prefix.clear();
                self.received = 0;
                self.frame_deadline = None;
                return Ok(std::mem::take(&mut self.payload));
            }
        }
    }
}

async fn stopped(shutdown: &mut Option<tokio::sync::watch::Receiver<bool>>) {
    if let Some(shutdown) = shutdown {
        let _ = shutdown.wait_for(|stopping| *stopping).await;
    } else {
        std::future::pending::<()>().await;
    }
}

async fn receive<S: AsyncRead + Unpin>(
    stream: &mut S,
    shutdown: &mut Option<tokio::sync::watch::Receiver<bool>>,
    deadline: Instant,
    buffer: &mut [u8],
) -> io::Result<usize> {
    tokio::select! {
        biased;
        _ = stopped(shutdown) => Err(io::Error::new(io::ErrorKind::Interrupted, "server stopping")),
        result = timeout_at(deadline, stream.read(buffer)) => result.map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "network frame deadline exceeded"))?,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;
    use tokio::time::{advance, timeout};

    #[tokio::test(start_paused = true)]
    async fn cancelled_reads_preserve_prefix_body_and_next_frame() {
        let (mut peer, socket) = duplex(512);
        let mut connection = Connection::new(socket);
        peer.write_all(&[0x80]).await.unwrap();
        assert!(timeout(Duration::from_secs(1), connection.read_frame())
            .await
            .is_err());
        peer.write_all(&[0x01]).await.unwrap();
        peer.write_all(&[42; 50]).await.unwrap();
        assert!(timeout(Duration::from_secs(1), connection.read_frame())
            .await
            .is_err());
        peer.write_all(&[42; 78]).await.unwrap();
        peer.write_all(&[1, 7]).await.unwrap();
        assert_eq!(connection.read_frame().await.unwrap(), vec![42; 128]);
        assert_eq!(connection.read_frame().await.unwrap(), vec![7]);
    }

    #[tokio::test(start_paused = true)]
    async fn fragmented_frame_cannot_restart_its_deadline() {
        let (mut peer, socket) = duplex(16);
        let mut connection = Connection::new(socket);
        connection.enter_play();
        peer.write_all(&[3, 1]).await.unwrap();
        assert!(timeout(Duration::from_secs(20), connection.read_frame())
            .await
            .is_err());
        advance(Duration::from_secs(11)).await;
        peer.write_all(&[2, 3]).await.unwrap();
        assert!(
            matches!(connection.read_frame().await, Err(PacketError::Io(error)) if error.kind() == io::ErrorKind::TimedOut)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn setup_budget_is_total_across_successful_packets() {
        let (mut peer, socket) = duplex(16);
        let mut connection = Connection::new(socket);
        peer.write_all(&[1, 0]).await.unwrap();
        connection.read_frame().await.unwrap();
        advance(Duration::from_secs(31)).await;
        peer.write_all(&[1, 0]).await.unwrap();
        assert!(connection.read_frame().await.is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn stalled_reads_and_writes_expire() {
        let (_peer, socket) = duplex(16);
        let mut connection = Connection::new(socket);
        let started = Instant::now();
        assert!(
            matches!(connection.read_frame().await, Err(PacketError::Io(error)) if error.kind() == io::ErrorKind::TimedOut)
        );
        assert_eq!(Instant::now() - started, FRAME_TIMEOUT);
        connection.enter_play();
        let started = Instant::now();
        assert_eq!(
            connection.write_all(&[0; 64]).await.unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(Instant::now() - started, WRITE_TIMEOUT);
    }

    #[tokio::test(start_paused = true)]
    async fn packet_and_byte_floods_are_rejected() {
        let (mut peer, socket) = duplex(1024);
        let mut connection = Connection::new(socket);
        peer.write_all(&[1, 0].repeat(241)).await.unwrap();
        for _ in 0..240 {
            connection.read_frame().await.unwrap();
        }
        assert!(
            matches!(connection.read_frame().await, Err(PacketError::Io(error)) if error.to_string().contains("packet rate"))
        );
        let (mut peer, socket) = duplex(32);
        let mut connection = Connection::new(socket);
        connection.bytes = Budget::new(16, 1);
        peer.write_all(&[8, 0, 0, 0, 0, 0, 0, 0, 0].repeat(2))
            .await
            .unwrap();
        connection.read_frame().await.unwrap();
        assert!(
            matches!(connection.read_frame().await, Err(PacketError::Io(error)) if error.to_string().contains("byte rate"))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn budgets_refill_but_never_accumulate_beyond_burst() {
        let mut budget = Budget::new(128, 64);
        assert!(budget.take(128));
        assert!(!budget.take(1));
        advance(Duration::from_secs(1)).await;
        assert!(budget.take(64));
        assert!(!budget.take(1));
        advance(Duration::from_secs(100)).await;
        assert!(!budget.take(129));
        assert!(budget.take(128));
    }

    #[tokio::test]
    async fn malformed_lengths_fail_before_allocating_a_body() {
        for prefix in [
            vec![0],
            vec![0xff, 0xff, 0xff, 0xff, 0x0f],
            vec![0x80; 5],
            vec![0x81, 0x80, 0x80, 0x80, 0x10],
            vec![0x81, 0x80, 0x80, 0x01],
        ] {
            let (mut peer, socket) = duplex(16);
            let mut connection = Connection::new(socket);
            peer.write_all(&prefix).await.unwrap();
            assert!(connection.read_frame().await.is_err());
            assert!(connection.payload.is_empty());
            assert!(connection.prefix.len() <= 5);
        }
    }

    #[tokio::test]
    async fn truncated_frames_report_eof() {
        for bytes in [vec![], vec![0x80], vec![4, 1, 2]] {
            let (mut peer, socket) = duplex(16);
            let mut connection = Connection::new(socket);
            peer.write_all(&bytes).await.unwrap();
            drop(peer);
            assert!(
                matches!(connection.read_frame().await, Err(PacketError::Io(error)) if error.kind() == io::ErrorKind::UnexpectedEof)
            );
        }
    }

    #[tokio::test]
    async fn shutdown_interrupts_partial_reads_and_blocked_writes() {
        let (tx, rx) = tokio::sync::watch::channel(false);
        let (_peer, socket) = duplex(16);
        let mut connection = Connection::new(socket);
        connection.observe_shutdown(rx);
        tx.send(true).unwrap();
        assert!(
            matches!(connection.read_frame().await, Err(PacketError::Io(error)) if error.kind() == io::ErrorKind::Interrupted)
        );
        assert_eq!(
            connection.write_all(&[0; 64]).await.unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
    }

    #[test]
    fn admission_caps_all_sources_and_releases_empty_entries() {
        let admissions = Admissions::default();
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        let mut leases = Vec::new();
        for _ in 0..MAX_CONNECTIONS_PER_IP {
            leases.push(admissions.acquire(ip).unwrap());
        }
        assert!(admissions.acquire(ip).is_none());
        assert!(admissions
            .acquire("::ffff:127.0.0.1".parse().unwrap())
            .is_none());
        leases.pop();
        leases.push(admissions.acquire(ip).unwrap());
        for number in 1..=(MAX_CONNECTIONS - MAX_CONNECTIONS_PER_IP) {
            leases.push(
                admissions
                    .acquire(IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, number as u8)))
                    .unwrap(),
            );
        }
        assert!(admissions.acquire("192.0.2.1".parse().unwrap()).is_none());
        drop(leases);
        assert_eq!(admissions.0.lock().unwrap().total, 0);
        assert!(admissions.0.lock().unwrap().ips.is_empty());
        assert!(admissions.acquire(ip).is_some());
    }
}
