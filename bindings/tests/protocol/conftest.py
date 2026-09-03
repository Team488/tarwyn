import os
import pathlib
import socket
import subprocess
import time

import pytest

NT4_PORT = 5810
HOST = "127.0.0.1"


def _server_binary():
    override = os.environ.get("TARWYN_SERVER_BIN")
    if override:
        return pathlib.Path(override)
    root = pathlib.Path(__file__).resolve().parents[3]
    return root / "target" / "release" / "tarwyn_server"


def _wait_for_port(port, timeout=20.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        with socket.socket() as s:
            s.settimeout(0.2)
            if s.connect_ex((HOST, port)) == 0:
                return True
        time.sleep(0.05)
    return False


@pytest.fixture(scope="session")
def server():
    binary = _server_binary()
    if not binary.exists():
        pytest.skip(f"no server binary at {binary}; build it or set TARWYN_SERVER_BIN")
    proc = subprocess.Popen([str(binary)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if not _wait_for_port(NT4_PORT):
        proc.kill()
        pytest.fail(f"server did not open {NT4_PORT}")
    yield f"{HOST}:{NT4_PORT}"
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()


@pytest.fixture
def nt_client(server):
    import ntcore

    created = []

    def connect(name):
        inst = ntcore.NetworkTableInstance.create()
        inst.startClient4(name)
        inst.setServer(HOST, NT4_PORT)
        deadline = time.time() + 20
        while time.time() < deadline and not inst.isConnected():
            time.sleep(0.05)
        assert inst.isConnected(), f"{name} never connected to {server}"
        created.append(inst)
        return inst

    yield connect
    for inst in created:
        inst.stopClient()
        ntcore.NetworkTableInstance.destroy(inst)
