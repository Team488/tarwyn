"""Fixtures shared by the Python client's tests."""

import pytest

import tarwyn

# Ports nothing is bound to, so every test runs against an absent server.
OFFLINE = ("127.0.0.1", 26982, 26983, 26981, 26984, 150, 500)


@pytest.fixture
def client():
    # The generated client releases through __del__ rather than an explicit
    # close, so dropping the reference is the whole teardown.
    yield tarwyn.TarwynClient.with_ports(*OFFLINE)
