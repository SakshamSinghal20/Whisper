import json
from pathlib import Path

import pytest
from coincurve import PrivateKey

from whisper.core import CoreError, ScanKey, transaction_tweak

VECTORS = json.loads((Path(__file__).parent / "fixtures/bip352.json").read_text())
RECEIVERS = [(case["comment"], recv) for case in VECTORS for recv in case["receiving"]]


@pytest.mark.parametrize("name,vector", RECEIVERS, ids=[item[0] for item in RECEIVERS])
def test_published_receiving_vectors(name, vector):
    given, expected = vector["given"], vector["expected"]
    scan = ScanKey(bytes.fromhex(given["key_material"]["scan_priv_key"]))
    spend = PrivateKey(bytes.fromhex(given["key_material"]["spend_priv_key"])).public_key.format()
    tweak = transaction_tweak(given["vin"])
    if expected.get("tweak"):
        assert tweak.hex() == expected["tweak"]
    if tweak is None:
        assert not expected.get("outputs", [])
        assert expected.get("n_outputs", 0) == 0
        return
    if "shared_secret" in expected:
        assert scan.shared_secret(tweak).hex() == expected["shared_secret"]
    outputs = [bytes.fromhex(key) for key in given["outputs"]]
    matches = scan.scan_outputs(tweak, spend, outputs, given["labels"])
    if "outputs" in expected:
        actual = {(match.output_pubkey.hex(), match.tweak.hex()) for match in matches}
        wanted = {(item["pub_key"], item["priv_key_tweak"]) for item in expected["outputs"]}
        assert actual == wanted
    else:
        assert len(matches) == expected["n_outputs"]


def test_secrets_are_redacted():
    assert "010101" not in repr(ScanKey(bytes([1]) * 32))


def test_missing_prevout_fails_instead_of_silently_skipping():
    with pytest.raises(CoreError, match="missing prevouts"):
        transaction_tweak([{"txid": "11" * 32, "vout": 0}])


def test_reject_zero_scan_key():
    with pytest.raises(CoreError):
        ScanKey(bytes(32))
