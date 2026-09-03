import os
import pathlib
import socket
import subprocess
import time

import ntcore

BINARY_ENV = "TARWYN_SERVER_BIN"
HOST = "127.0.0.1"
NT4_PORT = 5810


def binary_path():
    override = os.environ.get(BINARY_ENV)
    if override:
        return pathlib.Path(override)
    root = pathlib.Path(__file__).resolve().parents[3]
    return root / "target" / "release" / "tarwyn_server"


def wait_for_port(port, timeout=20.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        with socket.socket() as sock:
            sock.settimeout(0.2)
            if sock.connect_ex((HOST, port)) == 0:
                return True
        time.sleep(0.05)
    return False


def start(binary):
    proc = subprocess.Popen([str(binary)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if not wait_for_port(NT4_PORT):
        proc.kill()
        return None
    return proc


def stop(proc):
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()


def connect(name, timeout=20.0):
    inst = ntcore.NetworkTableInstance.create()
    inst.startClient4(name)
    inst.setServer(HOST, NT4_PORT)
    deadline = time.time() + timeout
    while time.time() < deadline and not inst.isConnected():
        time.sleep(0.05)
    return inst


def disconnect(inst):
    inst.stopClient()
    ntcore.NetworkTableInstance.destroy(inst)
