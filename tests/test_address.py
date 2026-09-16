import pytest
from coincurve import PrivateKey

from whisper.address import SilentPaymentAddress
from whisper.core import CoreError, ScanKey

from .test_core import RECEIVERS


@pytest.mark.parametrize("name,vector", RECEIVERS, ids=[item[0] for item in RECEIVERS])
def test_published_addresses(name, vector):
    given = vector["given"]
    keys = given["key_material"]
    scan = ScanKey(bytes.fromhex(keys["scan_priv_key"]))
    spend = PrivateKey(bytes.fromhex(keys["spend_priv_key"])).public_key.format()
    for label, expected in zip(
        [None, *given["labels"]], vector["expected"]["addresses"], strict=True
    ):
        address = SilentPaymentAddress.from_keys(scan, spend, label)
        assert address.encode() == expected
        assert SilentPaymentAddress.decode(expected) == address


def test_checksum_and_mixed_case_rejected():
    address = RECEIVERS[0][1]["expected"]["addresses"][0]
    with pytest.raises(CoreError, match="checksum"):
        SilentPaymentAddress.decode(address[:-1] + "p")
    with pytest.raises(CoreError, match="case"):
        SilentPaymentAddress.decode("S" + address[1:])


def test_testnet_address_roundtrip():
    scan = ScanKey(bytes([1]) * 32)
    address = SilentPaymentAddress.from_keys(scan, scan.public, testnet=True)
    assert address.encode().startswith("tsp1q")
    assert SilentPaymentAddress.decode(address.encode().upper()) == address
