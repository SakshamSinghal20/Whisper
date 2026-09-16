# Third-party material

## BIP-352 test vectors

`tests/fixtures/bip352.json` is the unmodified public receiving/sending fixture from:

https://github.com/bitcoin/bips/blob/master/bip-0352/send_and_receive_test_vectors.json

Retrieved September 16, 2026. The associated BIP-352 specification identifies its license as BSD-2-Clause. The tests exercise receiving and address encoding/decoding; the presence of sending vectors does not mean Whisper implements a sending wallet.

## Outfit

`whisper/static/outfit.woff2` is the Outfit variable Latin font served by Google Fonts. The font project is available at https://github.com/Outfitio/Outfit-Fonts. Its SIL Open Font License is included at `whisper/static/OFL-Outfit.txt`.

## Dependencies

Python dependency versions are recorded in `requirements.lock`; each dependency retains its own license. Coincurve supplies native libsecp256k1 operations. Whisper's Python implementation does not require a Rust toolchain.
