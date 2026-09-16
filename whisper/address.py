"""Version-0 Silent Payment address encoding (BIP-352 / Bech32m)."""

from dataclasses import dataclass

from coincurve import PublicKey

from whisper.core import CoreError, ScanKey

CHARSET = "qpzry9x8gf2tvdw0s3jn54khce6mua7l"
GENERATORS = (0x3B6A57B2, 0x26508E6D, 0x1EA119FA, 0x3D4233DD, 0x2A1462B3)


def polymod(values):
    result = 1
    for value in values:
        top = result >> 25
        result = ((result & 0x1FFFFFF) << 5) ^ value
        for bit, generator in enumerate(GENERATORS):
            if (top >> bit) & 1:
                result ^= generator
    return result


def convert_bits(values, from_bits, to_bits, pad=True):
    result, accumulator, bits = [], 0, 0
    for value in values:
        if not 0 <= value < (1 << from_bits):
            raise CoreError("Invalid address data")
        accumulator = (accumulator << from_bits) | value
        bits += from_bits
        while bits >= to_bits:
            bits -= to_bits
            result.append((accumulator >> bits) & ((1 << to_bits) - 1))
        accumulator &= (1 << bits) - 1
    if pad and bits:
        result.append((accumulator << (to_bits - bits)) & ((1 << to_bits) - 1))
    elif not pad and (bits >= from_bits or accumulator):
        raise CoreError("Invalid address padding")
    return result


def expanded(hrp):
    return [ord(c) >> 5 for c in hrp] + [0] + [ord(c) & 31 for c in hrp]


@dataclass(frozen=True)
class SilentPaymentAddress:
    scan_pubkey: bytes
    spend_pubkey: bytes
    testnet: bool = False

    def __post_init__(self):
        for key in (self.scan_pubkey, self.spend_pubkey):
            if len(key) != 33 or key[0] not in (2, 3):
                raise CoreError("Silent Payment addresses require compressed public keys")
            PublicKey(key)

    def encode(self) -> str:
        hrp = "tsp" if self.testnet else "sp"
        data = [0, *convert_bits(self.scan_pubkey + self.spend_pubkey, 8, 5)]
        checksum = polymod(expanded(hrp) + data + [0] * 6) ^ 0x2BC830A3
        suffix = [(checksum >> (5 * (5 - i))) & 31 for i in range(6)]
        return hrp + "1" + "".join(CHARSET[value] for value in data + suffix)

    @classmethod
    def decode(cls, address: str):
        if not 8 <= len(address) <= 1023 or (
            address.lower() != address and address.upper() != address
        ):
            raise CoreError("Invalid address length or mixed case")
        hrp, separator, encoded = address.lower().rpartition("1")
        if not separator or hrp not in {"sp", "tsp"}:
            raise CoreError("Expected an sp or tsp Silent Payment address")
        try:
            values = [CHARSET.index(c) for c in encoded]
        except ValueError as exc:
            raise CoreError("Invalid address character") from exc
        if polymod(expanded(hrp) + values) != 0x2BC830A3:
            raise CoreError("Invalid address checksum")
        if values[0] != 0:
            raise CoreError("Only version-0 receiving addresses are supported")
        payload = bytes(convert_bits(values[1:-6], 5, 8, pad=False))
        if len(payload) != 66:
            raise CoreError("Invalid Silent Payment address payload")
        return cls(payload[:33], payload[33:], testnet=hrp == "tsp")

    @classmethod
    def from_keys(cls, scan_key: ScanKey, spend_pubkey: bytes, label=None, *, testnet=False):
        spend = PublicKey(spend_pubkey)
        if label is not None:
            spend = spend.add(scan_key.label_tweak(label))
        return cls(scan_key.public, spend.format(), testnet)
