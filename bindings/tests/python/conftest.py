import pytest

import tarwyn

OFFLINE = ("127.0.0.1", 26982, 26983, 26981, 26984, 150, 500)


@pytest.fixture
def client():
    yield tarwyn.TarwynClient.with_ports(*OFFLINE)
