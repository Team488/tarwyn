use std::net::UdpSocket;

pub const HEADER_LEN: usize = 16;
pub const MAX_DATAGRAM: usize = 65_507;
pub const DEFAULT_TELEMETRY_PORT: u16 = 5558;

pub fn topic_hash(channel: &str) -> u32 {
    let mut hash: u32 = 2_166_136_261;
    for byte in channel.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

pub fn encode(buf: &mut [u8], channel_hash: u32, timestamp_us: u64, payload: &[u8]) -> usize {
    let len = payload.len().min(buf.len().saturating_sub(HEADER_LEN));
    buf[0..4].copy_from_slice(&channel_hash.to_le_bytes());
    buf[4..12].copy_from_slice(&timestamp_us.to_le_bytes());
    buf[12..16].copy_from_slice(&(len as u32).to_le_bytes());
    buf[HEADER_LEN..HEADER_LEN + len].copy_from_slice(&payload[..len]);
    HEADER_LEN + len
}

pub fn decode(buf: &[u8]) -> Option<(u32, u64, &[u8])> {
    if buf.len() < HEADER_LEN {
        return None;
    }
    let channel_hash = u32::from_le_bytes(buf[0..4].try_into().ok()?);
    let timestamp_us = u64::from_le_bytes(buf[4..12].try_into().ok()?);
    let len = u32::from_le_bytes(buf[12..16].try_into().ok()?) as usize;
    if buf.len() < HEADER_LEN + len {
        return None;
    }
    Some((
        channel_hash,
        timestamp_us,
        &buf[HEADER_LEN..HEADER_LEN + len],
    ))
}

pub fn now_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

pub const SOCKET_BUFFER_BYTES: usize = 256 * 1024;

pub fn bind_ephemeral() -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    tune(&socket);
    Ok(socket)
}

pub fn tune(socket: &UdpSocket) {
    tune_with(socket, SOCKET_BUFFER_BYTES)
}

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
    fn short_buffers_are_rejected() {
        assert!(decode(&[0u8; 4]).is_none());
        let mut buf = [0u8; 32];
        let n = encode(&mut buf, 1, 2, b"abcd");
        assert!(decode(&buf[..n - 1]).is_none());
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
