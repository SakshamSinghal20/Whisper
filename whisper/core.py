"""BIP-352 receiver primitives using libsecp256k1 through coincurve.

Input dictionaries use Bitcoin Core's decoded transaction format with prevouts.
No scan secret or scan public key is needed by the indexer.
"""

import hashlib
from collections.abc import Iterable
from dataclasses import dataclass, field
from typing import Any

from coincurve import PrivateKey, PublicKey

ORDER = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
K_MAX = 2323
NUMS_H = bytes.fromhex("50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0")


class CoreError(ValueError):
    """Invalid or incomplete cryptographic input."""


def tagged_hash(tag: str, data: bytes) -> bytes:
    tag_hash = hashlib.sha256(tag.encode("ascii")).digest()
    return hashlib.sha256(tag_hash + tag_hash + data).digest()


def scalar(data: bytes) -> int:
    value = int.from_bytes(data, "big")
    if len(data) != 32 or not 0 < value < ORDER:
        raise CoreError("Expected a nonzero secp256k1 scalar")
    return value


def hash160(data: bytes) -> bytes:
    return hashlib.new("ripemd160", hashlib.sha256(data).digest()).digest()


def script_pushes(script: bytes) -> Iterable[bytes]:
    """Read pushes, including nonstandard P2PKH scriptSigs with other opcodes."""
    offset = 0
    while offset < len(script):
        opcode = script[offset]
        offset += 1
        if opcode <= 75:
            size = opcode
        elif opcode in (76, 77, 78):
            width = 1 << (opcode - 76)
            if offset + width > len(script):
                raise CoreError("Truncated script push length")
            size = int.from_bytes(script[offset : offset + width], "little")
            offset += width
        else:
            continue
        if offset + size > len(script):
            raise CoreError("Truncated script push")
        yield script[offset : offset + size]
        offset += size


def witness_items(value: list[str] | str) -> list[bytes]:
    if isinstance(value, list):
        return [bytes.fromhex(item) for item in value]
    # The published test vectors serialize the witness stack as hex.
    raw = bytes.fromhex(value)
    if not raw:
        return []
    offset = 0

    def compact_size() -> int:
        nonlocal offset
        if offset >= len(raw):
            raise CoreError("Truncated witness")
        first = raw[offset]
        offset += 1
        if first < 253:
            return first
        width = {253: 2, 254: 4, 255: 8}[first]
        if offset + width > len(raw):
            raise CoreError("Truncated witness length")
        result = int.from_bytes(raw[offset : offset + width], "little")
        offset += width
        return result

    count = compact_size()
    items = []
    for _ in range(count):
        size = compact_size()
        if offset + size > len(raw):
            raise CoreError("Truncated witness item")
        items.append(raw[offset : offset + size])
        offset += size
    if offset != len(raw):
        raise CoreError("Trailing witness bytes")
    return items


def input_pubkey(vin: dict[str, Any]) -> PublicKey | None:
    script = bytes.fromhex(vin["prevout"]["scriptPubKey"]["hex"])
    sig = vin.get("scriptSig", "")
    sig = bytes.fromhex(sig["hex"] if isinstance(sig, dict) else sig)
    witness = witness_items(vin.get("txinwitness", []))
    if len(script) == 34 and script[:2] == b"\x51\x20":
        if len(witness) > 1 and witness[-1][:1] == b"\x50":
            witness = witness[:-1]  # BIP-341 annex
        if len(witness) > 1:
            control = witness[-1]
            if len(control) >= 33 and control[1:33] == NUMS_H:
                return None
        return PublicKey(b"\x02" + script[2:])
    key = None
    if len(script) == 22 and script[:2] == b"\x00\x14" and len(witness) == 2:
        if hash160(witness[-1]) == script[2:]:
            key = witness[-1]
    elif len(script) == 23 and script[:2] == b"\xa9\x14" and script[-1:] == b"\x87":
        if len(sig) == 23 and sig[:3] == b"\x16\x00\x14" and len(witness) == 2:
            if hash160(sig[1:]) == script[2:22] and hash160(witness[-1]) == sig[3:]:
                key = witness[-1]
    elif len(script) == 25 and script[:3] == b"\x76\xa9\x14" and script[-2:] == b"\x88\xac":
        key = next((p for p in script_pushes(sig) if hash160(p) == script[3:23]), None)
    if key and len(key) == 33 and key[0] in (2, 3):
        return PublicKey(key)
    return None


def transaction_tweak(inputs: list[dict[str, Any]]) -> bytes | None:
    """Return input_hash * A, or None for an ineligible transaction.

    All inputs need prevouts, including ineligible inputs: their outpoints still
    participate in selecting the smallest serialized outpoint.
    """
    if not inputs or any("coinbase" in item for item in inputs):
        return None
    keys, outpoints = [], []
    try:
        for item in inputs:
            script = bytes.fromhex(item["prevout"]["scriptPubKey"]["hex"])
            if (
                4 <= len(script) <= 42
                and 0x52 <= script[0] <= 0x60
                and script[1] == len(script) - 2
            ):
                return None
            txid = bytes.fromhex(item["txid"])
            if len(txid) != 32:
                raise CoreError("Invalid input transaction ID")
            outpoints.append(txid[::-1] + item["vout"].to_bytes(4, "little"))
            key = input_pubkey(item)
            if key:
                keys.append(key)
        if not keys:
            return None
        try:
            aggregate = PublicKey.combine_keys(keys)
        except ValueError:
            return None  # Point at infinity is explicitly ineligible.
        digest = tagged_hash("BIP0352/Inputs", min(outpoints) + aggregate.format())
        scalar(digest)
        return aggregate.multiply(digest).format()
    except (KeyError, TypeError, OverflowError, ValueError) as exc:
        raise CoreError("Invalid transaction inputs or missing prevouts") from exc


@dataclass(frozen=True)
class OutputMatch:
    output_index: int
    output_pubkey: bytes
    tweak: bytes = field(repr=False)
    label: int | None
    k: int


@dataclass(frozen=True)
class ScanKey:
    secret: bytes = field(repr=False)

    def __post_init__(self) -> None:
        scalar(self.secret)

    @property
    def public(self) -> bytes:
        return PrivateKey(self.secret).public_key.format()

    def shared_secret(self, input_tweak: bytes) -> bytes:
        return PublicKey(input_tweak).multiply(self.secret).format()

    def label_tweak(self, label: int) -> bytes:
        if isinstance(label, bool) or not isinstance(label, int) or not 0 <= label < 2**32:
            raise CoreError("Label must be an unsigned 32-bit integer")
        result = tagged_hash("BIP0352/Label", self.secret + label.to_bytes(4, "big"))
        scalar(result)
        return result

    def derive_output(
        self, shared_secret: bytes, spend_pubkey: bytes, k: int = 0, label: int | None = None
    ) -> tuple[bytes, bytes]:
        if not 0 <= k < K_MAX:
            raise CoreError(f"Output counter must be between 0 and {K_MAX - 1}")
        if len(shared_secret) != 33 or len(spend_pubkey) != 33:
            raise CoreError("Shared secret and spend key must retain compressed key parity")
        PublicKey(shared_secret)
        tweak = scalar(tagged_hash("BIP0352/SharedSecret", shared_secret + k.to_bytes(4, "big")))
        if label is not None:
            tweak = (tweak + scalar(self.label_tweak(label))) % ORDER
        tweak_bytes = tweak.to_bytes(32, "big")
        spend = PublicKey(spend_pubkey)
        output = spend.add(tweak_bytes) if tweak else spend
        return output.format()[1:], tweak_bytes

    def scan_outputs(
        self,
        input_tweak: bytes,
        spend_pubkey: bytes,
        outputs: list[bytes],
        labels: Iterable[int] = (),
    ) -> list[OutputMatch]:
        """Match all sequential outputs, always checking the change label (0)."""
        shared = self.shared_secret(input_tweak)
        choices = [None, *dict.fromkeys([0, *labels])]
        remaining: dict[bytes, list[int]] = {}
        for index, output in enumerate(outputs):
            if len(output) != 32:
                raise CoreError("Output keys must be 32 bytes")
            remaining.setdefault(output, []).append(index)
        matches = []
        for k in range(min(len(outputs), K_MAX)):
            found = False
            for label in choices:
                key, tweak = self.derive_output(shared, spend_pubkey, k, label)
                if key in remaining:
                    # Duplicate output scripts are separate spendable outpoints.
                    for index in remaining.pop(key):
                        matches.append(OutputMatch(index, key, tweak, label, k))
                    found = True
                    break
            if not found:
                break
        return sorted(matches, key=lambda match: match.output_index)


def prefix_from_xonly(key: bytes) -> str:
    if len(key) != 32:
        raise CoreError("Expected a 32-byte x-only public key")
    return key[:4].hex()
