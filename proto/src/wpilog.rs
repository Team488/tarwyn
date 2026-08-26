use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::protobuf::supported_values::Kind;

const MAGIC: &[u8; 6] = b"WPILOG";
const VERSION: u16 = 0x0100;
const CONTROL_ENTRY: u64 = 0;
const CONTROL_START: u8 = 0;
const CONTROL_FINISH: u8 = 1;

pub const DEFAULT_QUEUE: usize = 8192;
pub const DEFAULT_FLUSH: Duration = Duration::from_millis(250);

pub fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_micros() as u64)
        .unwrap_or(0)
}

fn min_len(value: u64) -> usize {
    let mut len = 1;
    let mut rest = value >> 8;
    while rest > 0 {
        len += 1;
        rest >>= 8;
    }
    len
}

fn put_le(out: &mut Vec<u8>, value: u64, len: usize) {
    for byte in 0..len {
        out.push((value >> (8 * byte)) as u8);
    }
}

fn type_name(kind: &Kind) -> &'static str {
    match kind {
        Kind::String(_) => "string",
        Kind::Int32(_) | Kind::Int64(_) | Kind::Uint32(_) | Kind::Uint64(_) => "int64",
        Kind::Bool(_) => "boolean",
        Kind::Double(_) => "double",
        Kind::Float(_) => "float",
        Kind::Bytes(_) | Kind::BytesList(_) => "raw",
        Kind::StringList(_) => "string[]",
        Kind::FloatList(_) => "float[]",
        Kind::BoolList(_) => "boolean[]",
        Kind::DoubleList(_) | Kind::CoordinateList(_) => "double[]",
        Kind::IntegerList(_) | Kind::LongList(_) => "int64[]",
    }
}

fn encode(kind: &Kind, out: &mut Vec<u8>) {
    match kind {
        Kind::String(value) => out.extend_from_slice(value.as_bytes()),
        Kind::Int32(value) => out.extend_from_slice(&i64::from(*value).to_le_bytes()),
        Kind::Int64(value) => out.extend_from_slice(&value.to_le_bytes()),
        Kind::Uint32(value) => out.extend_from_slice(&i64::from(*value).to_le_bytes()),
        Kind::Uint64(value) => out.extend_from_slice(&(*value as i64).to_le_bytes()),
        Kind::Bool(value) => out.push(u8::from(*value)),
        Kind::Double(value) => out.extend_from_slice(&value.to_le_bytes()),
        Kind::Float(value) => out.extend_from_slice(&value.to_le_bytes()),
        Kind::Bytes(value) => out.extend_from_slice(value),
        Kind::BoolList(list) => {
            for value in &list.values {
                out.push(u8::from(*value));
            }
        }
        Kind::FloatList(list) => {
            for value in &list.values {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        Kind::StringList(list) => {
            out.extend_from_slice(&(list.values.len() as u32).to_le_bytes());
            for value in &list.values {
                out.extend_from_slice(&(value.len() as u32).to_le_bytes());
                out.extend_from_slice(value.as_bytes());
            }
        }
        Kind::BytesList(list) => {
            out.extend_from_slice(&(list.values.len() as u32).to_le_bytes());
            for value in &list.values {
                out.extend_from_slice(&(value.len() as u32).to_le_bytes());
                out.extend_from_slice(value);
            }
        }
        Kind::DoubleList(list) => {
            for value in &list.values {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        Kind::IntegerList(list) => {
            for value in &list.values {
                out.extend_from_slice(&i64::from(*value).to_le_bytes());
            }
        }
        Kind::LongList(list) => {
            for value in &list.values {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        Kind::CoordinateList(list) => {
            for coordinate in &list.coordinates {
                out.extend_from_slice(&coordinate.x.to_le_bytes());
                out.extend_from_slice(&coordinate.y.to_le_bytes());
            }
        }
    }
}

fn scan_mounts(root: &Path, depth: usize, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        out.push(path.clone());
        if depth > 0 {
            scan_mounts(&path, depth - 1, out);
        }
    }
}

pub fn removable_mounts() -> Vec<std::path::PathBuf> {
    let mut mounts = Vec::new();
    for root in ["/media", "/run/media", "/mnt"] {
        scan_mounts(Path::new(root), 1, &mut mounts);
    }
    mounts
}

struct Entry {
    id: u32,
    type_name: String,
}

pub struct Writer<W: Write> {
    out: W,
    entries: HashMap<String, Entry>,
    next_id: u32,
    scratch: Vec<u8>,
}

impl<W: Write> Writer<W> {
    pub fn new(out: W) -> io::Result<Self> {
        Self::with_extra_header(out, "")
    }

    pub fn with_extra_header(mut out: W, extra: &str) -> io::Result<Self> {
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        out.write_all(&(extra.len() as u32).to_le_bytes())?;
        out.write_all(extra.as_bytes())?;
        Ok(Self {
            out,
            entries: HashMap::new(),
            next_id: 1,
            scratch: Vec::with_capacity(256),
        })
    }

    fn write_record(&mut self, id: u64, timestamp: u64, payload: &[u8]) -> io::Result<()> {
        let id_len = min_len(id);
        let size_len = min_len(payload.len() as u64);
        let stamp_len = min_len(timestamp);

        let mut header = Vec::with_capacity(1 + id_len + size_len + stamp_len);
        header.push(((id_len - 1) | ((size_len - 1) << 2) | ((stamp_len - 1) << 4)) as u8);
        put_le(&mut header, id, id_len);
        put_le(&mut header, payload.len() as u64, size_len);
        put_le(&mut header, timestamp, stamp_len);

        self.out.write_all(&header)?;
        self.out.write_all(payload)
    }

    fn start_entry(&mut self, name: &str, type_name: &str, timestamp: u64) -> io::Result<u32> {
        let id = self.next_id;
        self.next_id += 1;

        let mut payload = Vec::with_capacity(name.len() + type_name.len() + 20);
        payload.push(CONTROL_START);
        payload.extend_from_slice(&id.to_le_bytes());
        payload.extend_from_slice(&(name.len() as u32).to_le_bytes());
        payload.extend_from_slice(name.as_bytes());
        payload.extend_from_slice(&(type_name.len() as u32).to_le_bytes());
        payload.extend_from_slice(type_name.as_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes());

        self.write_record(CONTROL_ENTRY, timestamp, &payload)?;
        self.entries.insert(
            name.to_string(),
            Entry {
                id,
                type_name: type_name.to_string(),
            },
        );
        Ok(id)
    }

    fn finish_entry(&mut self, id: u32, timestamp: u64) -> io::Result<()> {
        let mut payload = Vec::with_capacity(5);
        payload.push(CONTROL_FINISH);
        payload.extend_from_slice(&id.to_le_bytes());
        self.write_record(CONTROL_ENTRY, timestamp, &payload)
    }

    fn entry_id(&mut self, name: &str, type_name: &str, timestamp: u64) -> io::Result<u32> {
        match self.entries.get(name) {
            Some(entry) if entry.type_name == type_name => Ok(entry.id),
            Some(entry) => {
                let stale = entry.id;
                self.finish_entry(stale, timestamp)?;
                self.start_entry(name, type_name, timestamp)
            }
            None => self.start_entry(name, type_name, timestamp),
        }
    }

    pub fn append(&mut self, channel: &str, timestamp: u64, kind: &Kind) -> io::Result<()> {
        let id = self.entry_id(channel, type_name(kind), timestamp)?;
        let mut payload = std::mem::take(&mut self.scratch);
        payload.clear();
        encode(kind, &mut payload);
        let result = self.write_record(u64::from(id), timestamp, &payload);
        self.scratch = payload;
        result
    }

    pub fn append_raw(&mut self, channel: &str, timestamp: u64, payload: &[u8]) -> io::Result<()> {
        let id = self.entry_id(channel, "raw", timestamp)?;
        self.write_record(u64::from(id), timestamp, payload)
    }

    pub fn finish_all(&mut self, timestamp: u64) -> io::Result<()> {
        let ids: Vec<u32> = self.entries.values().map(|entry| entry.id).collect();
        self.entries.clear();
        for id in ids {
            self.finish_entry(id, timestamp)?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

enum Command {
    Value {
        channel: String,
        timestamp: u64,
        kind: Box<Kind>,
    },
    Raw {
        channel: String,
        timestamp: u64,
        payload: Vec<u8>,
    },
}

pub struct Logger {
    tx: Option<SyncSender<Command>>,
    dropped: Arc<AtomicU64>,
    failed: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Logger {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::with_capacity(path, DEFAULT_QUEUE, DEFAULT_FLUSH)
    }

    pub fn with_capacity(
        path: impl AsRef<Path>,
        queue: usize,
        flush_every: Duration,
    ) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;

        let writer = Writer::with_extra_header(BufWriter::new(file), "{\"source\":\"tarwyn\"}")?;

        let (tx, rx) = sync_channel(queue);
        let dropped = Arc::new(AtomicU64::new(0));
        let failed = Arc::new(AtomicBool::new(false));
        let thread_failed = Arc::clone(&failed);

        let handle = thread::spawn(move || drain(writer, rx, thread_failed, flush_every));

        Ok(Self {
            tx: Some(tx),
            dropped,
            failed,
            handle: Some(handle),
        })
    }

    pub fn open_on_drive(filename: &str) -> io::Result<(Self, std::path::PathBuf)> {
        let mounts = removable_mounts();
        let mut last = io::Error::new(io::ErrorKind::NotFound, "no writable removable drive");
        for mount in mounts {
            let path = mount.join(filename);
            match Self::open(&path) {
                Ok(logger) => return Ok((logger, path)),
                Err(error) => last = error,
            }
        }
        Err(last)
    }

    pub fn record(&self, channel: &str, kind: Kind) {
        self.submit(Command::Value {
            channel: channel.to_string(),
            timestamp: now_micros(),
            kind: Box::new(kind),
        });
    }

    pub fn record_raw(&self, channel: &str, payload: &[u8]) {
        self.submit(Command::Raw {
            channel: channel.to_string(),
            timestamp: now_micros(),
            payload: payload.to_vec(),
        });
    }

    fn submit(&self, command: Command) {
        let Some(tx) = self.tx.as_ref() else {
            return;
        };
        match tx.try_send(command) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn is_healthy(&self) -> bool {
        !self.failed.load(Ordering::Relaxed)
    }

    pub fn close(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.tx.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Logger {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn drain(
    mut writer: Writer<BufWriter<File>>,
    rx: Receiver<Command>,
    failed: Arc<AtomicBool>,
    flush_every: Duration,
) {
    let mut last_flush = Instant::now();

    loop {
        let command = match rx.recv_timeout(flush_every) {
            Ok(command) => Some(command),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };

        if let Some(command) = command
            && !failed.load(Ordering::Relaxed)
        {
            let wrote = match command {
                Command::Value {
                    channel,
                    timestamp,
                    kind,
                } => writer.append(&channel, timestamp, &kind),
                Command::Raw {
                    channel,
                    timestamp,
                    payload,
                } => writer.append_raw(&channel, timestamp, &payload),
            };
            if wrote.is_err() {
                failed.store(true, Ordering::Relaxed);
            }
        }

        if last_flush.elapsed() >= flush_every {
            if writer.flush().is_err() {
                failed.store(true, Ordering::Relaxed);
            }
            last_flush = Instant::now();
        }
    }

    let _ = writer.finish_all(now_micros());
    let _ = writer.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protobuf::{BoolList, StringList};

    struct Reader<'a> {
        data: &'a [u8],
        pos: usize,
    }

    impl<'a> Reader<'a> {
        fn new(data: &'a [u8]) -> Self {
            assert_eq!(&data[0..6], MAGIC);
            assert_eq!(u16::from_le_bytes([data[6], data[7]]), VERSION);
            let extra = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
            Self {
                data,
                pos: 12 + extra,
            }
        }

        fn take(&mut self, len: usize) -> u64 {
            let mut value = 0u64;
            for byte in 0..len {
                value |= u64::from(self.data[self.pos + byte]) << (8 * byte);
            }
            self.pos += len;
            value
        }

        fn next(&mut self) -> Option<(u64, u64, &'a [u8])> {
            if self.pos >= self.data.len() {
                return None;
            }
            let bitfield = self.data[self.pos];
            self.pos += 1;
            let id_len = (bitfield & 0b11) as usize + 1;
            let size_len = ((bitfield >> 2) & 0b11) as usize + 1;
            let stamp_len = ((bitfield >> 4) & 0b111) as usize + 1;

            let id = self.take(id_len);
            let size = self.take(size_len) as usize;
            let timestamp = self.take(stamp_len);
            let payload = &self.data[self.pos..self.pos + size];
            self.pos += size;
            Some((id, timestamp, payload))
        }
    }

    fn start_name(payload: &[u8]) -> (u32, String, String) {
        assert_eq!(payload[0], CONTROL_START);
        let id = u32::from_le_bytes(payload[1..5].try_into().unwrap());
        let name_len = u32::from_le_bytes(payload[5..9].try_into().unwrap()) as usize;
        let name = String::from_utf8(payload[9..9 + name_len].to_vec()).unwrap();
        let rest = 9 + name_len;
        let type_len = u32::from_le_bytes(payload[rest..rest + 4].try_into().unwrap()) as usize;
        let type_name = String::from_utf8(payload[rest + 4..rest + 4 + type_len].to_vec()).unwrap();
        (id, name, type_name)
    }

    fn write(values: &[(&str, Kind)]) -> Vec<u8> {
        let mut writer = Writer::new(Vec::new()).unwrap();
        for (channel, kind) in values {
            writer.append(channel, 1_000, kind).unwrap();
        }
        writer.flush().unwrap();
        writer.out
    }

    #[test]
    fn header_is_a_valid_wpilog_header() {
        let writer = Writer::with_extra_header(Vec::new(), "hi").unwrap();
        let data = writer.out;
        assert_eq!(&data[0..6], b"WPILOG");
        assert_eq!(u16::from_le_bytes([data[6], data[7]]), 0x0100);
        assert_eq!(u32::from_le_bytes(data[8..12].try_into().unwrap()), 2);
        assert_eq!(&data[12..14], b"hi");
    }

    #[test]
    fn a_value_writes_a_start_record_then_a_data_record() {
        let data = write(&[("/pose/x", Kind::Double(1.5))]);
        let mut reader = Reader::new(&data);

        let (id, _, payload) = reader.next().unwrap();
        assert_eq!(id, CONTROL_ENTRY);
        let (entry_id, name, type_name) = start_name(payload);
        assert_eq!(name, "/pose/x");
        assert_eq!(type_name, "double");

        let (id, timestamp, payload) = reader.next().unwrap();
        assert_eq!(id, u64::from(entry_id));
        assert_eq!(timestamp, 1_000);
        assert_eq!(f64::from_le_bytes(payload.try_into().unwrap()), 1.5);

        assert!(reader.next().is_none());
    }

    #[test]
    fn a_repeated_channel_starts_only_one_entry() {
        let data = write(&[
            ("/n", Kind::Int64(1)),
            ("/n", Kind::Int64(2)),
            ("/n", Kind::Int64(3)),
        ]);
        let mut reader = Reader::new(&data);
        let mut starts = 0;
        let mut values = Vec::new();
        while let Some((id, _, payload)) = reader.next() {
            if id == CONTROL_ENTRY {
                starts += 1;
            } else {
                values.push(i64::from_le_bytes(payload.try_into().unwrap()));
            }
        }
        assert_eq!(starts, 1);
        assert_eq!(values, vec![1, 2, 3]);
    }

    #[test]
    fn a_type_change_finishes_the_old_entry_and_starts_a_new_one() {
        let data = write(&[("/x", Kind::Double(1.0)), ("/x", Kind::Bool(true))]);
        let mut reader = Reader::new(&data);

        let (_, _, payload) = reader.next().unwrap();
        let (first_id, _, first_type) = start_name(payload);
        assert_eq!(first_type, "double");
        reader.next().unwrap();

        let (id, _, payload) = reader.next().unwrap();
        assert_eq!(id, CONTROL_ENTRY);
        assert_eq!(payload[0], CONTROL_FINISH);
        assert_eq!(
            u32::from_le_bytes(payload[1..5].try_into().unwrap()),
            first_id
        );

        let (_, _, payload) = reader.next().unwrap();
        let (second_id, _, second_type) = start_name(payload);
        assert_eq!(second_type, "boolean");
        assert_ne!(second_id, first_id);
    }

    #[test]
    fn integers_widen_to_int64_and_lists_encode_elementwise() {
        let data = write(&[
            ("/i", Kind::Int32(-7)),
            (
                "/b",
                Kind::BoolList(BoolList {
                    values: vec![true, false, true],
                }),
            ),
            (
                "/s",
                Kind::StringList(StringList {
                    values: vec!["ab".into(), "c".into()],
                }),
            ),
        ]);
        let mut reader = Reader::new(&data);
        let mut payloads = Vec::new();
        let mut types = Vec::new();
        while let Some((id, _, payload)) = reader.next() {
            if id == CONTROL_ENTRY {
                types.push(start_name(payload).2);
            } else {
                payloads.push(payload.to_vec());
            }
        }

        assert_eq!(types, vec!["int64", "boolean[]", "string[]"]);
        assert_eq!(
            i64::from_le_bytes(payloads[0].clone().try_into().unwrap()),
            -7
        );
        assert_eq!(payloads[1], vec![1, 0, 1]);
        assert_eq!(
            payloads[2],
            [
                &2u32.to_le_bytes()[..],
                &2u32.to_le_bytes()[..],
                b"ab",
                &1u32.to_le_bytes()[..],
                b"c",
            ]
            .concat()
        );
    }

    #[test]
    fn a_large_timestamp_widens_the_header_fields() {
        let mut writer = Writer::new(Vec::new()).unwrap();
        writer
            .append("/t", now_micros(), &Kind::Bool(true))
            .unwrap();
        writer.flush().unwrap();
        let data = writer.out;

        let mut reader = Reader::new(&data);
        reader.next().unwrap();
        let (_, timestamp, payload) = reader.next().unwrap();
        assert!(timestamp > 1_600_000_000_000_000);
        assert_eq!(payload, [1]);
    }

    #[test]
    fn a_short_final_record_stays_visible_to_the_wpilib_iterator() {
        let path = std::env::temp_dir().join("tarwyn_wpilog_tail.wpilog");
        {
            let logger = Logger::open(&path).unwrap();
            logger.record("/a", Kind::Int64(1));
            logger.record_raw("/b", &[1, 2, 3, 4]);
            logger.close();
        }

        let data = std::fs::read(&path).unwrap();
        let mut reader = Reader::new(&data);
        let mut last_data_start = None;
        loop {
            let start = reader.pos;
            match reader.next() {
                Some((id, _, _)) if id != CONTROL_ENTRY => last_data_start = Some(start),
                Some(_) => {}
                None => break,
            }
        }

        let last_data_start = last_data_start.expect("no data records were written");
        assert!(
            last_data_start + 16 <= data.len(),
            "last data record starts at {last_data_start} of {} bytes, so DataLogIterator skips it",
            data.len()
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn the_logger_writes_a_readable_file_and_counts_drops() {
        let path = std::env::temp_dir().join("tarwyn_wpilog_test.wpilog");
        {
            let logger = Logger::open(&path).unwrap();
            for value in 0..64 {
                logger.record("/count", Kind::Int64(value));
            }
            assert!(logger.is_healthy());
            logger.close();
        }

        let data = std::fs::read(&path).unwrap();
        let mut reader = Reader::new(&data);
        let mut values = Vec::new();
        while let Some((id, _, payload)) = reader.next() {
            if id != CONTROL_ENTRY {
                values.push(i64::from_le_bytes(payload.try_into().unwrap()));
            }
        }
        assert_eq!(values, (0..64).collect::<Vec<_>>());
        assert!(data.len() >= 16);
        std::fs::remove_file(&path).ok();
    }
}
