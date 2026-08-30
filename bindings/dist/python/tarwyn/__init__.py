from __future__ import annotations


from dataclasses import dataclass




from collections.abc import Sequence




import struct


import sys
import uuid
from pathlib import Path

from . import _native


def _shared_library_filename() -> str:
    if sys.platform == "win32":
        return "tarwyn_bindings.dll"
    if sys.platform == "darwin":
        return "libtarwyn_bindings.dylib"
    return "libtarwyn_bindings.so"


_native._initialize_loader(str(Path(__file__).resolve().with_name(_shared_library_filename())))



_BOLTFFI_STRUCT_I8 = struct.Struct("<b")
_BOLTFFI_STRUCT_U8 = struct.Struct("<B")
_BOLTFFI_STRUCT_I16 = struct.Struct("<h")
_BOLTFFI_STRUCT_U16 = struct.Struct("<H")
_BOLTFFI_STRUCT_I32 = struct.Struct("<i")
_BOLTFFI_STRUCT_U32 = struct.Struct("<I")
_BOLTFFI_STRUCT_I64 = struct.Struct("<q")
_BOLTFFI_STRUCT_U64 = struct.Struct("<Q")
_BOLTFFI_STRUCT_F32 = struct.Struct("<f")
_BOLTFFI_STRUCT_F64 = struct.Struct("<d")

_BOLTFFI_UNPACK_I8 = _BOLTFFI_STRUCT_I8.unpack_from
_BOLTFFI_UNPACK_I16 = _BOLTFFI_STRUCT_I16.unpack_from
_BOLTFFI_UNPACK_U16 = _BOLTFFI_STRUCT_U16.unpack_from
_BOLTFFI_UNPACK_I32 = _BOLTFFI_STRUCT_I32.unpack_from
_BOLTFFI_UNPACK_U32 = _BOLTFFI_STRUCT_U32.unpack_from
_BOLTFFI_UNPACK_I64 = _BOLTFFI_STRUCT_I64.unpack_from
_BOLTFFI_UNPACK_U64 = _BOLTFFI_STRUCT_U64.unpack_from
_BOLTFFI_UNPACK_F32 = _BOLTFFI_STRUCT_F32.unpack_from
_BOLTFFI_UNPACK_F64 = _BOLTFFI_STRUCT_F64.unpack_from


def _boltffi_u32(value: int) -> bytes:
    return _BOLTFFI_STRUCT_U32.pack(int(value))


def _boltffi_wire_bool(value: bool) -> bytes:
    return b"\x01" if value else b"\x00"


def _boltffi_wire_i8(value: int) -> bytes:
    return _BOLTFFI_STRUCT_I8.pack(int(value))


def _boltffi_wire_u8(value: int) -> bytes:
    return _BOLTFFI_STRUCT_U8.pack(int(value))


def _boltffi_wire_i16(value: int) -> bytes:
    return _BOLTFFI_STRUCT_I16.pack(int(value))


def _boltffi_wire_u16(value: int) -> bytes:
    return _BOLTFFI_STRUCT_U16.pack(int(value))


def _boltffi_wire_i32(value: int) -> bytes:
    return _BOLTFFI_STRUCT_I32.pack(int(value))


def _boltffi_wire_u32(value: int) -> bytes:
    return _BOLTFFI_STRUCT_U32.pack(int(value))


def _boltffi_wire_i64(value: int) -> bytes:
    return _BOLTFFI_STRUCT_I64.pack(int(value))


def _boltffi_wire_u64(value: int) -> bytes:
    return _BOLTFFI_STRUCT_U64.pack(int(value))


def _boltffi_wire_isize(value: int) -> bytes:
    return _BOLTFFI_STRUCT_I64.pack(int(value))


def _boltffi_wire_usize(value: int) -> bytes:
    return _BOLTFFI_STRUCT_U64.pack(int(value))


def _boltffi_wire_f32(value: float) -> bytes:
    return _BOLTFFI_STRUCT_F32.pack(float(value))


def _boltffi_wire_f64(value: float) -> bytes:
    return _BOLTFFI_STRUCT_F64.pack(float(value))


def _boltffi_wire_string(value: str) -> bytes:
    payload = value.encode("utf-8")
    return _boltffi_u32(len(payload)) + payload


def _boltffi_wire_bytes(value: bytes) -> bytes:
    payload = bytes(value)
    return _boltffi_u32(len(payload)) + payload


def _boltffi_split_duration(value: float) -> tuple[int, int]:
    total = float(value)
    if total < 0:
        raise ValueError("duration must be non-negative")
    seconds = int(total)
    nanos = round((total - seconds) * 1_000_000_000)
    if nanos == 1_000_000_000:
        return seconds + 1, 0
    return seconds, nanos


def _boltffi_split_system_time(value: float) -> tuple[int, int]:
    total = float(value)
    seconds = int(total // 1)
    nanos = round((total - seconds) * 1_000_000_000)
    if nanos == 1_000_000_000:
        return seconds + 1, 0
    return seconds, nanos


def _boltffi_wire_duration(value: float) -> bytes:
    seconds, nanos = _boltffi_split_duration(value)
    return seconds.to_bytes(8, "little", signed=False) + nanos.to_bytes(4, "little", signed=False)


def _boltffi_wire_system_time(value: float) -> bytes:
    seconds, nanos = _boltffi_split_system_time(value)
    return seconds.to_bytes(8, "little", signed=True) + nanos.to_bytes(4, "little", signed=False)


def _boltffi_wire_uuid(value: uuid.UUID | str) -> bytes:
    raw = uuid.UUID(str(value)).bytes
    high = int.from_bytes(raw[:8], "big")
    low = int.from_bytes(raw[8:], "big")
    return high.to_bytes(8, "little", signed=False) + low.to_bytes(8, "little", signed=False)


def _boltffi_wire_url(value: str) -> bytes:
    return _boltffi_wire_string(str(value))


def _boltffi_wire_optional(value, encode) -> bytes:
    if value is None:
        return b"\x00"
    return b"\x01" + encode(value)


def _boltffi_wire_result(value, encode_ok, encode_err) -> bytes:
    ok, payload = value
    if ok:
        return b"\x00" + encode_ok(payload)
    return b"\x01" + encode_err(payload)


def _boltffi_wire_sequence(value, count, encode) -> bytes:
    items = list(value)
    if len(items) != count:
        raise ValueError("invalid BoltFFI sequence count")
    return _boltffi_u32(count) + b"".join(encode(item) for item in items)


def _boltffi_wire_map(value, encode_key, encode_value) -> bytes:
    items = list(value.items())
    return _boltffi_u32(len(items)) + b"".join(
        encode_key(key) + encode_value(item) for key, item in items
    )


def _boltffi_enum_value(value, enum_type, enum_name: str) -> int:
    if not isinstance(value, enum_type):
        raise TypeError(f"expected {enum_name}")
    return int(value)


def _boltffi_error_exception(error):
    for error_type in type(error).__mro__:
        exception_type = globals().get(f"{error_type.__name__}Exception")
        if exception_type is not None:
            return exception_type(error)
    return RuntimeError(error)


def _boltffi_call(error_decoder, call):
    try:
        return call()
    except RuntimeError as error:
        if error.args and isinstance(error.args[0], bytes):
            raise _boltffi_error_exception(error_decoder(error.args[0])) from error
        raise


class _BoltFfiWireReader:
    __slots__ = ("_data", "_offset")

    def __init__(self, data: bytes) -> None:
        self._data = data
        self._offset = 0

    def finish(self) -> None:
        if self._offset != len(self._data):
            raise ValueError("trailing BoltFFI wire bytes")

    def read(self, count: int) -> bytes:
        offset = self._offset
        end = offset + count
        if end > len(self._data):
            raise ValueError("truncated BoltFFI wire bytes")
        self._offset = end
        return self._data[offset:end]

    def bool(self) -> bool:
        value = self.u8()
        if value > 1:
            raise ValueError("invalid BoltFFI bool")
        return value == 1

    def i8(self) -> int:
        offset = self._offset
        self._offset = offset + 1
        return _BOLTFFI_UNPACK_I8(self._data, offset)[0]

    def u8(self) -> int:
        offset = self._offset
        if offset >= len(self._data):
            raise ValueError("truncated BoltFFI wire bytes")
        self._offset = offset + 1
        return self._data[offset]

    def i16(self) -> int:
        offset = self._offset
        self._offset = offset + 2
        return _BOLTFFI_UNPACK_I16(self._data, offset)[0]

    def u16(self) -> int:
        offset = self._offset
        self._offset = offset + 2
        return _BOLTFFI_UNPACK_U16(self._data, offset)[0]

    def i32(self) -> int:
        offset = self._offset
        self._offset = offset + 4
        return _BOLTFFI_UNPACK_I32(self._data, offset)[0]

    def u32(self) -> int:
        offset = self._offset
        self._offset = offset + 4
        return _BOLTFFI_UNPACK_U32(self._data, offset)[0]

    def i64(self) -> int:
        offset = self._offset
        self._offset = offset + 8
        return _BOLTFFI_UNPACK_I64(self._data, offset)[0]

    def u64(self) -> int:
        offset = self._offset
        self._offset = offset + 8
        return _BOLTFFI_UNPACK_U64(self._data, offset)[0]

    def isize(self) -> int:
        return self.i64()

    def usize(self) -> int:
        return self.u64()

    def f32(self) -> float:
        offset = self._offset
        self._offset = offset + 4
        return _BOLTFFI_UNPACK_F32(self._data, offset)[0]

    def f64(self) -> float:
        offset = self._offset
        self._offset = offset + 8
        return _BOLTFFI_UNPACK_F64(self._data, offset)[0]

    def string(self) -> str:
        count = self.u32()
        offset = self._offset
        end = offset + count
        if end > len(self._data):
            raise ValueError("truncated BoltFFI wire bytes")
        self._offset = end
        return str(memoryview(self._data)[offset:end], "utf-8")

    def bytes(self) -> bytes:
        return self.read(self.u32())

    def fixed(self, layout) -> tuple:
        offset = self._offset
        self._offset = offset + layout.size
        return layout.unpack_from(self._data, offset)

    def fixed_sequence(self, layout, factory) -> list:
        count = self.u32()
        offset = self._offset
        end = offset + count * layout.size
        if end > len(self._data):
            raise ValueError("truncated BoltFFI wire bytes")
        self._offset = end
        window = memoryview(self._data)[offset:end]
        return [factory(*values) for values in layout.iter_unpack(window)]

    def enum_sequence(self, layout, enum_type) -> list:
        count = self.u32()
        offset = self._offset
        end = offset + count * layout.size
        if end > len(self._data):
            raise ValueError("truncated BoltFFI wire bytes")
        self._offset = end
        window = memoryview(self._data)[offset:end]
        members = enum_type._value2member_map_
        try:
            return [members[value] for (value,) in layout.iter_unpack(window)]
        except KeyError as error:
            raise ValueError(f"invalid {enum_type.__name__} value") from error

    def duration(self) -> float:
        return self.u64() + self.u32() / 1_000_000_000

    def system_time(self) -> float:
        return self.i64() + self.u32() / 1_000_000_000

    def uuid(self) -> uuid.UUID:
        high = self.u64().to_bytes(8, "big", signed=False)
        low = self.u64().to_bytes(8, "big", signed=False)
        return uuid.UUID(bytes=high + low)

    def url(self) -> str:
        return self.string()

    def optional(self, decode):
        tag = self.u8()
        if tag == 0:
            return None
        if tag == 1:
            return decode()
        raise ValueError("invalid BoltFFI option tag")

    def result(self, decode_ok, decode_err):
        tag = self.u8()
        if tag == 0:
            return (True, decode_ok())
        if tag == 1:
            return (False, decode_err())
        raise ValueError("invalid BoltFFI result tag")

    def sequence(self, decode) -> list:
        return [decode() for _ in range(self.u32())]

    def map(self, decode_key, decode_value) -> dict:
        return {decode_key(): decode_value() for _ in range(self.u32())}


def _boltffi_read_wire(data: bytes, decode):
    reader = _BoltFfiWireReader(data)
    try:
        value = decode(reader)
    except struct.error as error:
        raise ValueError("truncated BoltFFI wire bytes") from error
    reader.finish()
    return value



def _boltffi_read_4319db60c88eabca(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.optional(lambda: reader.string()))


_native._register_wire_codec("read_4319db60c88eabca", _boltffi_read_4319db60c88eabca)


def _boltffi_read_bf04f784f44bd44e(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.optional(lambda: reader.bytes()))


_native._register_wire_codec("read_bf04f784f44bd44e", _boltffi_read_bf04f784f44bd44e)


def _boltffi_read_474076beb2cdf762(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.optional(lambda: reader.sequence(lambda: reader.string())))


_native._register_wire_codec("read_474076beb2cdf762", _boltffi_read_474076beb2cdf762)


def _boltffi_read_c32b92e2de8b1fe6(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.optional(lambda: reader.sequence(lambda: reader.bytes())))


_native._register_wire_codec("read_c32b92e2de8b1fe6", _boltffi_read_c32b92e2de8b1fe6)


def _boltffi_read_59a5b2def4d9c96f(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.optional(lambda: reader.sequence(lambda: reader.f64())))


_native._register_wire_codec("read_59a5b2def4d9c96f", _boltffi_read_59a5b2def4d9c96f)


def _boltffi_read_80ca419fa84ec288(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.optional(lambda: reader.sequence(lambda: reader.f32())))


_native._register_wire_codec("read_80ca419fa84ec288", _boltffi_read_80ca419fa84ec288)


def _boltffi_read_42d4b38d92702e46(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.optional(lambda: reader.sequence(lambda: reader.i32())))


_native._register_wire_codec("read_42d4b38d92702e46", _boltffi_read_42d4b38d92702e46)


def _boltffi_read_04df257b7c919a04(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.optional(lambda: reader.sequence(lambda: reader.i64())))


_native._register_wire_codec("read_04df257b7c919a04", _boltffi_read_04df257b7c919a04)


def _boltffi_read_e5e45e7271a24fe3(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.optional(lambda: reader.sequence(lambda: reader.bool())))


_native._register_wire_codec("read_e5e45e7271a24fe3", _boltffi_read_e5e45e7271a24fe3)


def _boltffi_read_046cf4fa65d40404(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.optional(lambda: reader.fixed_sequence(_BOLTFFI_STRUCT_Coordinate, Coordinate)))


_native._register_wire_codec("read_046cf4fa65d40404", _boltffi_read_046cf4fa65d40404)


def _boltffi_read_c6141593326a0c0e(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.optional(lambda: Pose2d._boltffi_from_reader(reader)))


_native._register_wire_codec("read_c6141593326a0c0e", _boltffi_read_c6141593326a0c0e)


def _boltffi_read_a62eb969835d5d39(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.optional(lambda: Pose3d._boltffi_from_reader(reader)))


_native._register_wire_codec("read_a62eb969835d5d39", _boltffi_read_a62eb969835d5d39)


def _boltffi_read_ef1975730da70106(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.optional(lambda: reader.sequence(lambda: Point._boltffi_from_reader(reader))))


_native._register_wire_codec("read_ef1975730da70106", _boltffi_read_ef1975730da70106)


def _boltffi_read_88fe13077020b58c(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.sequence(lambda: reader.string()))


_native._register_wire_codec("read_88fe13077020b58c", _boltffi_read_88fe13077020b58c)


def _boltffi_read_30d5e5ea816616cc(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.optional(lambda: ServerStatistics._boltffi_from_reader(reader)))


_native._register_wire_codec("read_30d5e5ea816616cc", _boltffi_read_30d5e5ea816616cc)


def _boltffi_read_89cd31291d2aefa4(data: bytes):
    return _boltffi_read_wire(data, lambda reader: reader.string())


_native._register_wire_codec("read_89cd31291d2aefa4", _boltffi_read_89cd31291d2aefa4)



def _boltffi_write_76ae0c6ac04e5071(host) -> bytes:
    return _boltffi_wire_string(host)


_native._register_wire_codec("write_76ae0c6ac04e5071", _boltffi_write_76ae0c6ac04e5071)


def _boltffi_write_732ffdd26d0208e0(channel) -> bytes:
    return _boltffi_wire_string(channel)


_native._register_wire_codec("write_732ffdd26d0208e0", _boltffi_write_732ffdd26d0208e0)


def _boltffi_write_ed06f1a2bac0816e(value) -> bytes:
    return _boltffi_wire_string(value)


_native._register_wire_codec("write_ed06f1a2bac0816e", _boltffi_write_ed06f1a2bac0816e)


def _boltffi_write_711bd57e8f0358ea(value) -> bytes:
    return _boltffi_wire_bytes(value)


_native._register_wire_codec("write_711bd57e8f0358ea", _boltffi_write_711bd57e8f0358ea)


def _boltffi_write_2543ca2b41e673b8(value) -> bytes:
    return _boltffi_wire_sequence(value, len(value), lambda __boltffi_value_0: _boltffi_wire_string(__boltffi_value_0))


_native._register_wire_codec("write_2543ca2b41e673b8", _boltffi_write_2543ca2b41e673b8)


def _boltffi_write_a12ee64f6da39c3c(value) -> bytes:
    return _boltffi_wire_sequence(value, len(value), lambda __boltffi_value_0: _boltffi_wire_bytes(__boltffi_value_0))


_native._register_wire_codec("write_a12ee64f6da39c3c", _boltffi_write_a12ee64f6da39c3c)


def _boltffi_write_f53851ecca04863c(value) -> bytes:
    return _boltffi_wire_sequence(value, len(value), lambda __boltffi_value_0: __boltffi_value_0._boltffi_wire())


_native._register_wire_codec("write_f53851ecca04863c", _boltffi_write_f53851ecca04863c)


def _boltffi_write_1aaa6a28ba312ce3(prefix) -> bytes:
    return _boltffi_wire_string(prefix)


_native._register_wire_codec("write_1aaa6a28ba312ce3", _boltffi_write_1aaa6a28ba312ce3)


def _boltffi_write_9b9ff1988bd4dbd3(expected) -> bytes:
    return _boltffi_wire_string(expected)


_native._register_wire_codec("write_9b9ff1988bd4dbd3", _boltffi_write_9b9ff1988bd4dbd3)


def _boltffi_write_7804c1c6d92ba96b(payload) -> bytes:
    return _boltffi_wire_bytes(payload)


_native._register_wire_codec("write_7804c1c6d92ba96b", _boltffi_write_7804c1c6d92ba96b)


def _boltffi_write_766cdeb069dd2b0a(path) -> bytes:
    return _boltffi_wire_string(path)


_native._register_wire_codec("write_766cdeb069dd2b0a", _boltffi_write_766cdeb069dd2b0a)


def _boltffi_write_2ef9e1074ce88f94(filename) -> bytes:
    return _boltffi_wire_string(filename)


_native._register_wire_codec("write_2ef9e1074ce88f94", _boltffi_write_2ef9e1074ce88f94)




@dataclass(frozen=True, slots=True)
class ServerStatistics:
    """Server counters, as reported by [`TarwynClient::get_server_statistics`]."""
    channels: int
    values: int
    telemetry_subscribers: int
    uptime_seconds: int
    dropped_publishes: int
    dropped_logs: int
    version: str

    def _boltffi_wire(self) -> bytes:
        return b"".join((
            _boltffi_wire_u64(self.channels),
            _boltffi_wire_u64(self.values),
            _boltffi_wire_u64(self.telemetry_subscribers),
            _boltffi_wire_u64(self.uptime_seconds),
            _boltffi_wire_u64(self.dropped_publishes),
            _boltffi_wire_u64(self.dropped_logs),
            _boltffi_wire_string(self.version),
        ))

    @classmethod
    def _boltffi_from_wire(cls, data: bytes) -> "ServerStatistics":
        reader = _BoltFfiWireReader(data)
        try:
            value = cls._boltffi_from_reader(reader)
        except struct.error as error:
            raise ValueError("truncated BoltFFI wire bytes") from error
        reader.finish()
        return value

    @classmethod
    def _boltffi_from_reader(cls, reader: "_BoltFfiWireReader") -> "ServerStatistics":
        return cls(
            channels=reader.u64(),
            values=reader.u64(),
            telemetry_subscribers=reader.u64(),
            uptime_seconds=reader.u64(),
            dropped_publishes=reader.u64(),
            dropped_logs=reader.u64(),
            version=reader.string(),
        )


_native._register_server_statistics(ServerStatistics)



Coordinate = _native.Coordinate
Coordinate.__module__ = __name__
Coordinate.__doc__ = """An `(x, y)` pair, as carried by the coordinate list type."""
Coordinate.__match_args__ = ("x","y",)
Coordinate.__annotations__ = {"x": float, "y": float}
_BOLTFFI_STRUCT_Coordinate = struct.Struct("<dd")


def _boltffi_attach_Coordinate_wire(self) -> bytes:
    return _BOLTFFI_STRUCT_Coordinate.pack(self.x, self.y)


def _boltffi_attach_Coordinate_from_wire(cls, data: bytes) -> "Coordinate":
    reader = _BoltFfiWireReader(data)
    try:
        value = cls._boltffi_from_reader(reader)
    except struct.error as error:
        raise ValueError("truncated BoltFFI wire bytes") from error
    reader.finish()
    return value


def _boltffi_attach_Coordinate_from_reader(cls, reader: "_BoltFfiWireReader") -> "Coordinate":
    return cls(*reader.fixed(_BOLTFFI_STRUCT_Coordinate))


Coordinate._boltffi_wire = _boltffi_attach_Coordinate_wire
Coordinate._boltffi_from_wire = classmethod(_boltffi_attach_Coordinate_from_wire)
Coordinate._boltffi_from_reader = classmethod(_boltffi_attach_Coordinate_from_reader)



@dataclass(frozen=True, slots=True)
class Point:
    """One control point of a bezier curve. `rotation_degrees` is absent for a point
    that does not constrain heading.
    """
    x: float
    y: float
    rotation_degrees: float | None

    def _boltffi_wire(self) -> bytes:
        return b"".join((
            _boltffi_wire_f64(self.x),
            _boltffi_wire_f64(self.y),
            _boltffi_wire_optional(self.rotation_degrees, lambda __boltffi_value_0: _boltffi_wire_f64(__boltffi_value_0)),
        ))

    @classmethod
    def _boltffi_from_wire(cls, data: bytes) -> "Point":
        reader = _BoltFfiWireReader(data)
        try:
            value = cls._boltffi_from_reader(reader)
        except struct.error as error:
            raise ValueError("truncated BoltFFI wire bytes") from error
        reader.finish()
        return value

    @classmethod
    def _boltffi_from_reader(cls, reader: "_BoltFfiWireReader") -> "Point":
        return cls(
            x=reader.f64(),
            y=reader.f64(),
            rotation_degrees=reader.optional(lambda: reader.f64()),
        )


_native._register_point(Point)



Pose2d = _native.Pose2d
Pose2d.__module__ = __name__
Pose2d.__doc__ = """A pose on the field plane."""
Pose2d.__match_args__ = ("x","y","rotation",)
Pose2d.__annotations__ = {"x": float, "y": float, "rotation": float}
_BOLTFFI_STRUCT_Pose2d = struct.Struct("<ddd")


def _boltffi_attach_Pose2d_wire(self) -> bytes:
    return _BOLTFFI_STRUCT_Pose2d.pack(self.x, self.y, self.rotation)


def _boltffi_attach_Pose2d_from_wire(cls, data: bytes) -> "Pose2d":
    reader = _BoltFfiWireReader(data)
    try:
        value = cls._boltffi_from_reader(reader)
    except struct.error as error:
        raise ValueError("truncated BoltFFI wire bytes") from error
    reader.finish()
    return value


def _boltffi_attach_Pose2d_from_reader(cls, reader: "_BoltFfiWireReader") -> "Pose2d":
    return cls(*reader.fixed(_BOLTFFI_STRUCT_Pose2d))


Pose2d._boltffi_wire = _boltffi_attach_Pose2d_wire
Pose2d._boltffi_from_wire = classmethod(_boltffi_attach_Pose2d_from_wire)
Pose2d._boltffi_from_reader = classmethod(_boltffi_attach_Pose2d_from_reader)



Pose3d = _native.Pose3d
Pose3d.__module__ = __name__
Pose3d.__doc__ = """A pose in space."""
Pose3d.__match_args__ = ("x","y","z","roll","pitch","yaw",)
Pose3d.__annotations__ = {"x": float, "y": float, "z": float, "roll": float, "pitch": float, "yaw": float}
_BOLTFFI_STRUCT_Pose3d = struct.Struct("<dddddd")


def _boltffi_attach_Pose3d_wire(self) -> bytes:
    return _BOLTFFI_STRUCT_Pose3d.pack(self.x, self.y, self.z, self.roll, self.pitch, self.yaw)


def _boltffi_attach_Pose3d_from_wire(cls, data: bytes) -> "Pose3d":
    reader = _BoltFfiWireReader(data)
    try:
        value = cls._boltffi_from_reader(reader)
    except struct.error as error:
        raise ValueError("truncated BoltFFI wire bytes") from error
    reader.finish()
    return value


def _boltffi_attach_Pose3d_from_reader(cls, reader: "_BoltFfiWireReader") -> "Pose3d":
    return cls(*reader.fixed(_BOLTFFI_STRUCT_Pose3d))


Pose3d._boltffi_wire = _boltffi_attach_Pose3d_wire
Pose3d._boltffi_from_wire = classmethod(_boltffi_attach_Pose3d_from_wire)
Pose3d._boltffi_from_reader = classmethod(_boltffi_attach_Pose3d_from_reader)



@dataclass(frozen=True, slots=True)
class Update:
    """A value published to a channel, delivered to a subscriber.

    The payload is the encoded value; `channel` names what it arrived on, so one
    subscription can carry several channels.
    """
    channel: str
    value: bytes

    def _boltffi_wire(self) -> bytes:
        return b"".join((
            _boltffi_wire_string(self.channel),
            _boltffi_wire_bytes(self.value),
        ))

    @classmethod
    def _boltffi_from_wire(cls, data: bytes) -> "Update":
        reader = _BoltFfiWireReader(data)
        try:
            value = cls._boltffi_from_reader(reader)
        except struct.error as error:
            raise ValueError("truncated BoltFFI wire bytes") from error
        reader.finish()
        return value

    @classmethod
    def _boltffi_from_reader(cls, reader: "_BoltFfiWireReader") -> "Update":
        return cls(
            channel=reader.string(),
            value=reader.bytes(),
        )


_native._register_update(Update)



@dataclass(frozen=True, slots=True)
class Telemetry:
    """A telemetry datagram, with the publisher's clock."""
    timestamp_micros: int
    payload: bytes

    def _boltffi_wire(self) -> bytes:
        return b"".join((
            _boltffi_wire_u64(self.timestamp_micros),
            _boltffi_wire_bytes(self.payload),
        ))

    @classmethod
    def _boltffi_from_wire(cls, data: bytes) -> "Telemetry":
        reader = _BoltFfiWireReader(data)
        try:
            value = cls._boltffi_from_reader(reader)
        except struct.error as error:
            raise ValueError("truncated BoltFFI wire bytes") from error
        reader.finish()
        return value

    @classmethod
    def _boltffi_from_reader(cls, reader: "_BoltFfiWireReader") -> "Telemetry":
        return cls(
            timestamp_micros=reader.u64(),
            payload=reader.bytes(),
        )


_native._register_telemetry(Telemetry)




class TarwynClient:
    __slots__ = ("_handle",)



    def __init__(self) -> None:
        """Connect to a server on localhost with the default ports."""
        self._handle = _native._boltffi_x_tables_client_new()



    @classmethod
    def _from_handle(cls, handle: int) -> "TarwynClient":
        value = cls.__new__(cls)
        value._handle = handle
        return value

    def __del__(self) -> None:
        handle = getattr(self, "_handle", None)
        if handle is not None:
            self._handle = None
            _native._boltffi_x_tables_client_release(handle)

    @classmethod
    def connect(cls, host: str) -> "TarwynClient":
        """Connect to a server on another machine - a coprocessor, or the robot controller."""
        return TarwynClient._from_handle(_native._boltffi_x_tables_client_connect(host))

    @classmethod
    def with_ports(cls, host: str, push_port: int, req_port: int, sub_port: int, telemetry_port: int, request_timeout_ms: int, send_high_water_mark: int) -> "TarwynClient":
        """Connect with every port and the request timeout spelled out."""
        return TarwynClient._from_handle(_native._boltffi_x_tables_client_with_ports(host, push_port, req_port, sub_port, telemetry_port, request_timeout_ms, send_high_water_mark))

    def start(self) -> None:
        """Start the receive threads, so subscriptions begin delivering.

        Publishing and reading work without this.
        """
        _native._boltffi_x_tables_client_start(self._handle)

    def stop(self) -> None:
        """Stop the receive threads. Subscriptions survive and resume on the next start."""
        _native._boltffi_x_tables_client_stop(self._handle)

    def put_string(self, channel: str, value: str) -> None:
        """Publish a string."""
        _native._boltffi_x_tables_client_put_string(self._handle, channel, value)

    def put_integer(self, channel: str, value: int) -> None:
        """Publish a 32-bit signed integer."""
        _native._boltffi_x_tables_client_put_integer(self._handle, channel, value)

    def put_long(self, channel: str, value: int) -> None:
        """Publish a 64-bit signed integer."""
        _native._boltffi_x_tables_client_put_long(self._handle, channel, value)

    def put_double(self, channel: str, value: float) -> None:
        """Publish a double."""
        _native._boltffi_x_tables_client_put_double(self._handle, channel, value)

    def put_float(self, channel: str, value: float) -> None:
        """Publish a float."""
        _native._boltffi_x_tables_client_put_float(self._handle, channel, value)

    def put_boolean(self, channel: str, value: bool) -> None:
        """Publish a boolean."""
        _native._boltffi_x_tables_client_put_boolean(self._handle, channel, value)

    def put_bytes(self, channel: str, value: bytes) -> None:
        """Publish raw bytes."""
        _native._boltffi_x_tables_client_put_bytes(self._handle, channel, _boltffi_wire_bytes(value))

    def put_string_list(self, channel: str, value: Sequence[str]) -> None:
        """Publish a list of strings."""
        _native._boltffi_x_tables_client_put_string_list(self._handle, channel, _boltffi_wire_sequence(value, len(value), lambda __boltffi_value_0: _boltffi_wire_string(__boltffi_value_0)))

    def put_bytes_list(self, channel: str, value: Sequence[bytes]) -> None:
        """Publish a list of byte strings."""
        _native._boltffi_x_tables_client_put_bytes_list(self._handle, channel, _boltffi_wire_sequence(value, len(value), lambda __boltffi_value_0: _boltffi_wire_bytes(__boltffi_value_0)))

    def put_double_list(self, channel: str, value: Sequence[float]) -> None:
        """Publish a list of doubles."""
        _native._boltffi_x_tables_client_put_double_list(self._handle, channel, value)

    def put_float_list(self, channel: str, value: Sequence[float]) -> None:
        """Publish a list of floats."""
        _native._boltffi_x_tables_client_put_float_list(self._handle, channel, value)

    def put_integer_list(self, channel: str, value: Sequence[int]) -> None:
        """Publish a list of 32-bit integers."""
        _native._boltffi_x_tables_client_put_integer_list(self._handle, channel, value)

    def put_long_list(self, channel: str, value: Sequence[int]) -> None:
        """Publish a list of 64-bit integers."""
        _native._boltffi_x_tables_client_put_long_list(self._handle, channel, value)

    def put_boolean_list(self, channel: str, value: Sequence[bool]) -> None:
        """Publish a list of booleans."""
        _native._boltffi_x_tables_client_put_boolean_list(self._handle, channel, value)

    def put_coordinates(self, channel: str, value: Sequence[Coordinate]) -> None:
        """Publish a list of `(x, y)` coordinates."""
        _native._boltffi_x_tables_client_put_coordinates(self._handle, channel, value)

    def put_pose2d(self, channel: str, value: Pose2d) -> None:
        """Publish a pose on the field plane."""
        _native._boltffi_x_tables_client_put_pose2d(self._handle, channel, value)

    def put_pose3d(self, channel: str, value: Pose3d) -> None:
        """Publish a pose in space."""
        _native._boltffi_x_tables_client_put_pose3d(self._handle, channel, value)

    def put_bezier_curve(self, channel: str, value: Sequence[Point]) -> None:
        """Publish one bezier curve."""
        _native._boltffi_x_tables_client_put_bezier_curve(self._handle, channel, _boltffi_wire_sequence(value, len(value), lambda __boltffi_value_0: __boltffi_value_0._boltffi_wire()))

    def put_bezier_curves(self, channel: str, value: bytes) -> bool:
        """Publish a bezier path already encoded as protobuf, byte-identical to TARWYN'."""
        return _native._boltffi_x_tables_client_put_bezier_curves(self._handle, channel, _boltffi_wire_bytes(value))

    def put_bezier_curves_list(self, channel: str, value: bytes) -> bool:
        """Publish several bezier paths, encoded as protobuf."""
        return _native._boltffi_x_tables_client_put_bezier_curves_list(self._handle, channel, _boltffi_wire_bytes(value))

    def put_unknown_bytes(self, channel: str, value: bytes) -> None:
        """Publish bytes whose type the caller does not know."""
        _native._boltffi_x_tables_client_put_unknown_bytes(self._handle, channel, _boltffi_wire_bytes(value))

    def put_typed_bytes(self, channel: str, tarwyn_type: int, value: bytes) -> bool:
        """Publish a value already encoded in TARWYN' byte layout, given its type tag.

        Returns false, publishing nothing, when a recognised tag comes with bytes
        that are not a valid value of that type.
        """
        return _native._boltffi_x_tables_client_put_typed_bytes(self._handle, channel, tarwyn_type, _boltffi_wire_bytes(value))

    def get_string(self, channel: str) -> str | None:
        """Read a string. Absent if the channel holds nothing, or another type."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_string(self._handle, channel), lambda reader: reader.optional(lambda: reader.string()))

    def get_integer(self, channel: str) -> int | None:
        """Read a 32-bit signed integer."""
        return _native._boltffi_x_tables_client_get_integer(self._handle, channel)

    def get_long(self, channel: str) -> int | None:
        """Read a 64-bit signed integer."""
        return _native._boltffi_x_tables_client_get_long(self._handle, channel)

    def get_double(self, channel: str) -> float | None:
        """Read a double."""
        return _native._boltffi_x_tables_client_get_double(self._handle, channel)

    def get_float(self, channel: str) -> float | None:
        """Read a float."""
        return _native._boltffi_x_tables_client_get_float(self._handle, channel)

    def get_boolean(self, channel: str) -> bool | None:
        """Read a boolean."""
        return _native._boltffi_x_tables_client_get_boolean(self._handle, channel)

    def get_bytes(self, channel: str) -> bytes | None:
        """Read raw bytes."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_bytes(self._handle, channel), lambda reader: reader.optional(lambda: reader.bytes()))

    def get_string_list(self, channel: str) -> list[str] | None:
        """Read a list of strings."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_string_list(self._handle, channel), lambda reader: reader.optional(lambda: reader.sequence(lambda: reader.string())))

    def get_bytes_list(self, channel: str) -> list[bytes] | None:
        """Read a list of byte strings."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_bytes_list(self._handle, channel), lambda reader: reader.optional(lambda: reader.sequence(lambda: reader.bytes())))

    def get_double_list(self, channel: str) -> list[float] | None:
        """Read a list of doubles."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_double_list(self._handle, channel), lambda reader: reader.optional(lambda: reader.sequence(lambda: reader.f64())))

    def get_float_list(self, channel: str) -> list[float] | None:
        """Read a list of floats."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_float_list(self._handle, channel), lambda reader: reader.optional(lambda: reader.sequence(lambda: reader.f32())))

    def get_integer_list(self, channel: str) -> list[int] | None:
        """Read a list of 32-bit integers."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_integer_list(self._handle, channel), lambda reader: reader.optional(lambda: reader.sequence(lambda: reader.i32())))

    def get_long_list(self, channel: str) -> list[int] | None:
        """Read a list of 64-bit integers."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_long_list(self._handle, channel), lambda reader: reader.optional(lambda: reader.sequence(lambda: reader.i64())))

    def get_boolean_list(self, channel: str) -> list[bool] | None:
        """Read a list of booleans."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_boolean_list(self._handle, channel), lambda reader: reader.optional(lambda: reader.sequence(lambda: reader.bool())))

    def get_coordinates(self, channel: str) -> list[Coordinate] | None:
        """Read a coordinate list."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_coordinates(self._handle, channel), lambda reader: reader.optional(lambda: reader.fixed_sequence(_BOLTFFI_STRUCT_Coordinate, Coordinate)))

    def get_pose2d(self, channel: str) -> Pose2d | None:
        """Read a pose on the field plane."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_pose2d(self._handle, channel), lambda reader: reader.optional(lambda: Pose2d._boltffi_from_reader(reader)))

    def get_pose3d(self, channel: str) -> Pose3d | None:
        """Read a pose in space."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_pose3d(self._handle, channel), lambda reader: reader.optional(lambda: Pose3d._boltffi_from_reader(reader)))

    def get_bezier_curve(self, channel: str) -> list[Point] | None:
        """Read one bezier curve as its control points."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_bezier_curve(self._handle, channel), lambda reader: reader.optional(lambda: reader.sequence(lambda: Point._boltffi_from_reader(reader))))

    def get_bezier_curves(self, channel: str) -> bytes | None:
        """Read a bezier path as encoded protobuf, byte-identical to TARWYN'."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_bezier_curves(self._handle, channel), lambda reader: reader.optional(lambda: reader.bytes()))

    def get_bezier_curves_list(self, channel: str) -> bytes | None:
        """Read a list of bezier paths as encoded protobuf."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_bezier_curves_list(self._handle, channel), lambda reader: reader.optional(lambda: reader.bytes()))

    def get_unknown_bytes(self, channel: str) -> bytes | None:
        """Read a channel holding raw bytes whose type the caller does not know."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_unknown_bytes(self._handle, channel), lambda reader: reader.optional(lambda: reader.bytes()))

    def delete(self, channel: str) -> int:
        """Delete a channel. Returns how many were removed, 0 or 1."""
        return _native._boltffi_x_tables_client_delete(self._handle, channel)

    def delete_all(self) -> int:
        """Delete every channel. Returns how many were removed."""
        return _native._boltffi_x_tables_client_delete_all(self._handle)

    def get_tables(self, prefix: str) -> list[str]:
        """List the channel names beginning with `prefix`. Pass \"" for all of them."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_tables(self._handle, prefix), lambda reader: reader.sequence(lambda: reader.string()))

    def get_ping(self) -> int | None:
        """Round-trip time to the server in nanoseconds, absent if it does not answer."""
        return _native._boltffi_x_tables_client_get_ping(self._handle)

    def get_server_statistics(self) -> ServerStatistics | None:
        """Server counters. Absent if the server does not answer."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_get_server_statistics(self._handle), lambda reader: reader.optional(lambda: ServerStatistics._boltffi_from_reader(reader)))

    def get_raw_json(self, prefix: str) -> str:
        """The channels beginning with `prefix`, as a JSON document."""
        return _native._boltffi_x_tables_client_get_raw_json(self._handle, prefix)

    def compare_and_set_absent_string(self, channel: str, value: str) -> bool:
        """Set a channel to `value` only while it is empty, and report whether it swapped."""
        return _native._boltffi_x_tables_client_compare_and_set_absent_string(self._handle, channel, value)

    def compare_and_set_string(self, channel: str, expected: str, value: str) -> bool:
        """Set a channel to `value` only if it currently holds `expected`."""
        return _native._boltffi_x_tables_client_compare_and_set_string(self._handle, channel, expected, value)

    def compare_and_set_double(self, channel: str, expected: float, value: float) -> bool:
        """Set a channel to `value` only if it currently holds `expected`."""
        return _native._boltffi_x_tables_client_compare_and_set_double(self._handle, channel, expected, value)

    def compare_and_set_long(self, channel: str, expected: int, value: int) -> bool:
        """Set a channel to `value` only if it currently holds `expected`."""
        return _native._boltffi_x_tables_client_compare_and_set_long(self._handle, channel, expected, value)

    def compare_and_set_boolean(self, channel: str, expected: bool, value: bool) -> bool:
        """Set a channel to `value` only if it currently holds `expected`."""
        return _native._boltffi_x_tables_client_compare_and_set_boolean(self._handle, channel, expected, value)

    def publish_telemetry(self, channel: str, payload: bytes) -> None:
        """Publish on the UDP telemetry plane, which trades delivery guarantees for latency."""
        _native._boltffi_x_tables_client_publish_telemetry(self._handle, channel, _boltffi_wire_bytes(payload))

    def log_to(self, path: str) -> bool:
        """Mirror every published value into a WPILOG file."""
        return _native._boltffi_x_tables_client_log_to(self._handle, path)

    def log_to_drive(self, filename: str) -> str | None:
        """As `log_to`, onto the first writable removable mount. Returns the path chosen."""
        return _boltffi_read_wire(_native._boltffi_x_tables_client_log_to_drive(self._handle, filename), lambda reader: reader.optional(lambda: reader.string()))

    def dropped_log_records(self) -> int:
        """How many log records were dropped because the writer queue was full."""
        return _native._boltffi_x_tables_client_dropped_log_records(self._handle)

    def logging_healthy(self) -> bool:
        """Whether the log writer is still succeeding."""
        return _native._boltffi_x_tables_client_logging_healthy(self._handle)

    def dropped_publishes(self) -> int:
        """How many publishes were dropped rather than queued, across both transports."""
        return _native._boltffi_x_tables_client_dropped_publishes(self._handle)

    def subscribe(self, channel: str) -> bool:
        """Deliver every value published to `channel`.

        Values arrive as soon as they are published: the consumer is woken rather
        than polling, so delivery is not paced by an interval.
        """
        return _native._boltffi_x_tables_client_subscribe(self._handle, channel)

    def subscribe_telemetry(self, channel: str) -> bool:
        """Receive telemetry on `channel`. Absent if another channel already claimed
        this one's topic hash - a collision is refused rather than cross-wired.
        """
        return _native._boltffi_x_tables_client_subscribe_telemetry(self._handle, channel)

    def subscribe_to_logs(self) -> bool:
        """Deliver every log line the server emits."""
        return _native._boltffi_x_tables_client_subscribe_to_logs(self._handle)

    def updates(self) -> "TarwynClientUpdatesSubscription":
        """The stream every [`Self::subscribe`] call feeds."""
        return TarwynClientUpdatesSubscription._from_handle(_native.updates(self._handle))

    def telemetry(self) -> "TarwynClientTelemetrySubscription":
        """The stream every [`Self::subscribe_telemetry`] call feeds."""
        return TarwynClientTelemetrySubscription._from_handle(_native.telemetry(self._handle))

    def logs(self) -> "TarwynClientLogsSubscription":
        """The stream [`Self::subscribe_to_logs`] feeds."""
        return TarwynClientLogsSubscription._from_handle(_native.logs(self._handle))


class TarwynClientUpdatesSubscription:
    __slots__ = ("_handle",)

    def __init__(self) -> None:
        raise TypeError("TarwynClientUpdatesSubscription cannot be constructed directly")

    @classmethod
    def _from_handle(cls, handle: int) -> "TarwynClientUpdatesSubscription":
        value = cls.__new__(cls)
        value._handle = handle
        return value

    def __del__(self) -> None:
        handle = getattr(self, "_handle", None)
        if handle is not None:
            self._handle = None
            _native.updates_free(handle)

    def pop_batch(self, max_count: int = 16) -> list[Update]:
        data = _native.updates_pop_batch(self._require_handle(), max_count)
        return _boltffi_read_wire(data, lambda reader: reader.sequence(lambda: Update._boltffi_from_reader(reader))) if data else []

    def wait(self, timeout_milliseconds: int) -> int:
        return _native.updates_wait(self._require_handle(), timeout_milliseconds)

    def unsubscribe(self) -> None:
        handle = self._require_handle()
        self._handle = None
        _native.updates_unsubscribe(handle)
        _native.updates_free(handle)

    def _require_handle(self) -> int:
        handle = self._handle
        if handle is None:
            raise RuntimeError("stream subscription is closed")
        return handle


class TarwynClientTelemetrySubscription:
    __slots__ = ("_handle",)

    def __init__(self) -> None:
        raise TypeError("TarwynClientTelemetrySubscription cannot be constructed directly")

    @classmethod
    def _from_handle(cls, handle: int) -> "TarwynClientTelemetrySubscription":
        value = cls.__new__(cls)
        value._handle = handle
        return value

    def __del__(self) -> None:
        handle = getattr(self, "_handle", None)
        if handle is not None:
            self._handle = None
            _native.telemetry_free(handle)

    def pop_batch(self, max_count: int = 16) -> list[Telemetry]:
        data = _native.telemetry_pop_batch(self._require_handle(), max_count)
        return _boltffi_read_wire(data, lambda reader: reader.sequence(lambda: Telemetry._boltffi_from_reader(reader))) if data else []

    def wait(self, timeout_milliseconds: int) -> int:
        return _native.telemetry_wait(self._require_handle(), timeout_milliseconds)

    def unsubscribe(self) -> None:
        handle = self._require_handle()
        self._handle = None
        _native.telemetry_unsubscribe(handle)
        _native.telemetry_free(handle)

    def _require_handle(self) -> int:
        handle = self._handle
        if handle is None:
            raise RuntimeError("stream subscription is closed")
        return handle


class TarwynClientLogsSubscription:
    __slots__ = ("_handle",)

    def __init__(self) -> None:
        raise TypeError("TarwynClientLogsSubscription cannot be constructed directly")

    @classmethod
    def _from_handle(cls, handle: int) -> "TarwynClientLogsSubscription":
        value = cls.__new__(cls)
        value._handle = handle
        return value

    def __del__(self) -> None:
        handle = getattr(self, "_handle", None)
        if handle is not None:
            self._handle = None
            _native.logs_free(handle)

    def pop_batch(self, max_count: int = 16) -> list[str]:
        data = _native.logs_pop_batch(self._require_handle(), max_count)
        return _boltffi_read_wire(data, lambda reader: reader.sequence(lambda: reader.string())) if data else []

    def wait(self, timeout_milliseconds: int) -> int:
        return _native.logs_wait(self._require_handle(), timeout_milliseconds)

    def unsubscribe(self) -> None:
        handle = self._require_handle()
        self._handle = None
        _native.logs_unsubscribe(handle)
        _native.logs_free(handle)

    def _require_handle(self) -> int:
        handle = self._handle
        if handle is None:
            raise RuntimeError("stream subscription is closed")
        return handle







MODULE_NAME = "tarwyn"
PACKAGE_NAME = "tarwyn"
PACKAGE_VERSION = "0.1.0"

__all__ = [
    "MODULE_NAME",
    "PACKAGE_NAME",
    "PACKAGE_VERSION",
    "ServerStatistics",
    "Coordinate",
    "Point",
    "Pose2d",
    "Pose3d",
    "Update",
    "Telemetry",
    "TarwynClient",
    "TarwynClientUpdatesSubscription",
    "TarwynClientTelemetrySubscription",
    "TarwynClientLogsSubscription",
]
