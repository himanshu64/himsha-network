"""Tests for HimshaPublicKey."""

import pytest
from himsha_sdk.pubkey import HimshaPublicKey, PROGRAM_IDS, ProgramIds


def test_from_bytes_32():
    b = bytes(range(32))
    pk = HimshaPublicKey(b)
    assert pk.to_bytes() == b


def test_wrong_length_raises():
    with pytest.raises(ValueError, match="32 bytes"):
        HimshaPublicKey(bytes(31))
    with pytest.raises(ValueError, match="32 bytes"):
        HimshaPublicKey(bytes(33))


def test_base58_round_trip():
    b = bytes([42] * 32)
    pk = HimshaPublicKey(b)
    b58 = pk.to_base58()
    assert b58
    restored = HimshaPublicKey.from_base58(b58)
    assert restored == pk


def test_from_seed_deterministic():
    a = HimshaPublicKey.from_seed("my-seed")
    b = HimshaPublicKey.from_seed("my-seed")
    c = HimshaPublicKey.from_seed("different-seed")
    assert a == b
    assert a != c


def test_str_returns_base58():
    pk = HimshaPublicKey.from_seed("hello")
    assert str(pk) == pk.to_base58()


def test_program_ids_correct():
    assert PROGRAM_IDS.system       == HimshaPublicKey.from_seed("himsha::system_program")
    assert PROGRAM_IDS.token        == HimshaPublicKey.from_seed("himsha::token_program")
    assert PROGRAM_IDS.ata          == HimshaPublicKey.from_seed("himsha::ata_program")
    assert PROGRAM_IDS.swap         == HimshaPublicKey.from_seed("himsha::swap_program")
    assert PROGRAM_IDS.lending      == HimshaPublicKey.from_seed("himsha::lending_program")
    assert PROGRAM_IDS.nft_metadata == HimshaPublicKey.from_seed("himsha::nft_metadata_program")


def test_find_program_address_deterministic():
    program = HimshaPublicKey.from_seed("my-program")
    pda1, bump1 = HimshaPublicKey.find_program_address([b"vault"], program)
    pda2, bump2 = HimshaPublicKey.find_program_address([b"vault"], program)
    assert pda1 == pda2
    assert bump1 == bump2


def test_find_program_address_different_seeds():
    program = HimshaPublicKey.from_seed("prog")
    pda1, _ = HimshaPublicKey.find_program_address([b"vault"], program)
    pda2, _ = HimshaPublicKey.find_program_address([b"treasury"], program)
    assert pda1 != pda2


def test_equality_and_hash():
    pk1 = HimshaPublicKey.from_seed("same")
    pk2 = HimshaPublicKey.from_seed("same")
    pk3 = HimshaPublicKey.from_seed("other")
    assert pk1 == pk2
    assert pk1 != pk3
    assert hash(pk1) == hash(pk2)
    assert hash(pk1) != hash(pk3)

    # usable as dict key
    d = {pk1: "value"}
    assert d[pk2] == "value"
