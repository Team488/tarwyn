import os

import pytest

import nt4_server


@pytest.fixture(scope="session")
def server():
    binary = nt4_server.binary_path()
    if not binary.exists():
        if nt4_server.BINARY_ENV in os.environ:
            pytest.fail(f"{nt4_server.BINARY_ENV} points at {binary}, which does not exist")
        pytest.skip(f"no server binary at {binary}; build it or set {nt4_server.BINARY_ENV}")
    proc = nt4_server.start(binary)
    if proc is None:
        pytest.fail(f"server did not open {nt4_server.NT4_PORT}")
    yield f"{nt4_server.HOST}:{nt4_server.NT4_PORT}"
    nt4_server.stop(proc)


@pytest.fixture
def nt_client(server):
    connected = []

    def connect(name):
        inst = nt4_server.connect(name)
        assert inst.isConnected(), f"{name} never connected to {server}"
        connected.append(inst)
        return inst

    yield connect
    for inst in connected:
        nt4_server.disconnect(inst)
