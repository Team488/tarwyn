use std::net::UdpSocket;

/// Marks a datagram as belonging to this protocol, so foreign traffic that
/// lands on the port is discarded rather than parsed.
pub const MAGIC: u32 = 0x5854_0001;
/// Marks a datagram that asks the server to relay a channel back to whoever sent
/// it.
///
/// A subscriber registers by sending one of these rather than by naming an
/// address over the control plane: the server takes the destination from the
/// datagram's own source, which is the only address it can verify. That is also
/// the address NAT rewrites on the way, so it is the one that works.
pub const MAGIC_REGISTER: u32 = 0x5854_0002;
/// Bytes of header ahead of every payload.
pub const HEADER_LEN: usize = 20;
/// Largest payload a single datagram can carry, after the IP and UDP headers.
pub const MAX_DATAGRAM: usize = 65_507;
/// Default UDP port for the telemetry plane.
pub const DEFAULT_TELEMETRY_PORT: u16 = 4883;

/// The 32-bit FNV-1a hash a channel name travels under.
///
/// Datagrams carry this rather than the name, which keeps the header fixed-width.
/// Two names can collide; callers are expected to refuse the second name rather
/// than cross-wire the two channels.
pub fn topic_hash(channel: &str) -> u32 {
    let mut hash: u32 = 2_166_136_261;
    for byte in channel.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

/// Write a datagram into `buf` and return how many bytes it occupies.
///
/// A payload too large for `buf` is truncated rather than overflowing it, so
/// `buf` should be [`HEADER_LEN`] plus the payload length.
pub fn encode(buf: &mut [u8], channel_hash: u32, timestamp_us: u64, payload: &[u8]) -> usize {
    encode_with(buf, MAGIC, channel_hash, timestamp_us, payload)
}

fn encode_with(
    buf: &mut [u8],
    magic: u32,
    channel_hash: u32,
    timestamp_us: u64,
    payload: &[u8],
) -> usize {
    let len = payload.len().min(buf.len().saturating_sub(HEADER_LEN));
    buf[0..4].copy_from_slice(&magic.to_le_bytes());
    buf[4..8].copy_from_slice(&channel_hash.to_le_bytes());
    buf[8..16].copy_from_slice(&timestamp_us.to_le_bytes());
    buf[16..20].copy_from_slice(&(len as u32).to_le_bytes());
    buf[HEADER_LEN..HEADER_LEN + len].copy_from_slice(&payload[..len]);
    HEADER_LEN + len
}

/// Write a registration for `channel_hash` into `buf`, which must hold
/// [`HEADER_LEN`] bytes. Returns how many bytes it occupies.
pub fn encode_registration(buf: &mut [u8], channel_hash: u32) -> usize {
    encode_with(buf, MAGIC_REGISTER, channel_hash, now_micros(), &[])
}

/// Read a datagram, returning its channel hash, timestamp and payload.
///
/// `None` if the buffer is too short, carries the wrong [`MAGIC`], or claims a
/// length its bytes do not back. A registration is not a data datagram and is
/// rejected here; read it with [`decode_registration`].
pub fn decode(buf: &[u8]) -> Option<(u32, u64, &[u8])> {
    decode_with(buf, MAGIC)
}

/// Read a registration, returning the channel hash it asks for.
///
/// `None` for anything that is not a registration, so the same buffer can be
/// offered to this and to [`decode`] in either order.
pub fn decode_registration(buf: &[u8]) -> Option<u32> {
    decode_with(buf, MAGIC_REGISTER).map(|(channel_hash, _, _)| channel_hash)
}

fn decode_with(buf: &[u8], magic: u32) -> Option<(u32, u64, &[u8])> {
    if buf.len() < HEADER_LEN {
        return None;
    }
    if u32::from_le_bytes(buf[0..4].try_into().ok()?) != magic {
        return None;
    }
    let channel_hash = u32::from_le_bytes(buf[4..8].try_into().ok()?);
    let timestamp_us = u64::from_le_bytes(buf[8..16].try_into().ok()?);
    let len = u32::from_le_bytes(buf[16..20].try_into().ok()?) as usize;
    if len > MAX_DATAGRAM || buf.len() < HEADER_LEN + len {
        return None;
    }
    Some((
        channel_hash,
        timestamp_us,
        &buf[HEADER_LEN..HEADER_LEN + len],
    ))
}

/// Microseconds since the Unix epoch, or 0 if the clock is before it.
pub fn now_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

/// Send and receive buffer size [`tune`] asks the kernel for.
pub const SOCKET_BUFFER_BYTES: usize = 256 * 1024;

/// Bind a tuned UDP socket on a port the kernel chooses.
pub fn bind_ephemeral() -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    tune(&socket);
    Ok(socket)
}

/// Enlarge a socket's kernel buffers to [`SOCKET_BUFFER_BYTES`], so a burst is
/// absorbed rather than dropped.
pub fn tune(socket: &UdpSocket) {
    tune_with(socket, SOCKET_BUFFER_BYTES)
}

/// As [`tune`], with the buffer size spelled out. A no-op off Unix, and silent
/// if the kernel refuses the size.
pub fn tune_with(socket: &UdpSocket, bytes: usize) {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let fd = socket.as_raw_fd();
        let size = bytes as libc::c_int;
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVBUF,
                &size as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_SNDBUF,
                &size as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }
    }
    #[cfg(not(unix))]
    let _ = (socket, bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut buf = [0u8; 128];
        let n = encode(&mut buf, topic_hash("pose"), 1234, b"payload");
        let (hash, timestamp, body) = decode(&buf[..n]).unwrap();
        assert_eq!(hash, topic_hash("pose"));
        assert_eq!(timestamp, 1234);
        assert_eq!(body, b"payload");
    }

    #[test]
    fn foreign_datagrams_are_rejected() {
        let mut buf = [0u8; 64];
        let n = encode(&mut buf, 1, 2, b"payload");
        buf[0] ^= 0xFF;
        assert!(decode(&buf[..n]).is_none());
        assert!(decode(b"random garbage on the port").is_none());
    }

    #[test]
    fn absurd_lengths_are_rejected() {
        let mut buf = [0u8; 64];
        let n = encode(&mut buf, 1, 2, b"x");
        buf[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&buf[..n]).is_none());
    }

    #[test]
    fn short_buffers_are_rejected() {
        assert!(decode(&[0u8; 4]).is_none());
        let mut buf = [0u8; 32];
        let n = encode(&mut buf, 1, 2, b"abcd");
        assert!(decode(&buf[..n - 1]).is_none());
    }

    #[test]
    fn a_registration_is_not_mistaken_for_data() {
        let mut buf = [0u8; HEADER_LEN];
        let n = encode_registration(&mut buf, topic_hash("pose"));

        assert_eq!(decode_registration(&buf[..n]), Some(topic_hash("pose")));
        assert!(
            decode(&buf[..n]).is_none(),
            "a registration read as data would be relayed to every subscriber"
        );
    }

    #[test]
    fn data_is_not_mistaken_for_a_registration() {
        let mut buf = [0u8; 64];
        let n = encode(&mut buf, topic_hash("pose"), 1234, b"payload");

        assert!(
            decode_registration(&buf[..n]).is_none(),
            "a data datagram read as a registration would subscribe its publisher"
        );
    }

    #[test]
    fn hashes_differ_between_channels() {
        assert_ne!(topic_hash("pose"), topic_hash("tags"));
        assert_eq!(topic_hash("pose"), topic_hash("pose"));
    }

    #[test]
    fn payload_is_truncated_not_overflowed() {
        let mut buf = [0u8; HEADER_LEN + 4];
        let n = encode(&mut buf, 1, 2, b"much longer than four");
        let (_, _, body) = decode(&buf[..n]).unwrap();
        assert_eq!(body.len(), 4);
    }
}
