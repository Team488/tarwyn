import pytest
import tarwyn

OFFLINE = ("127.0.0.1", 26982, 26983, 26981, 26984, 150, 500)


class Discard:
    def update(self, update):
        pass


@pytest.fixture
def discard():
    yield Discard()


@pytest.fixture
def client():
    yield tarwyn.TarwynClient.with_ports(*OFFLINE)
