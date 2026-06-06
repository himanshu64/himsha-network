"""Tests for HimshaInstruction, HimshaMessage, HimshaTransaction."""

import pytest
from himsha_sdk.pubkey import HimshaPublicKey, PROGRAM_IDS
from himsha_sdk.transaction import AccountMeta, HimshaInstruction, HimshaMessage, HimshaTransaction


def key(seed: str) -> HimshaPublicKey:
    return HimshaPublicKey.from_seed(seed)


class TestAccountMeta:
    def test_writable(self):
        pk = key("acc")
        m = AccountMeta.writable(pk, is_signer=True)
        assert m.is_signer is True
        assert m.is_writable is True

    def test_readonly(self):
        pk = key("acc")
        m = AccountMeta.readonly(pk, is_signer=False)
        assert m.is_writable is False
        assert m.is_signer is False

    def test_to_json(self):
        pk = key("acc")
        m = AccountMeta.writable(pk, True)
        j = m.to_json()
        assert j["pubkey"] == pk.to_base58()
        assert j["isSigner"] is True
        assert j["isWritable"] is True


class TestHimshaInstruction:
    def test_fields(self):
        prog = key("prog")
        acc  = key("acc")
        data = bytes([1, 2, 3])
        ix = HimshaInstruction(
            program_id=prog,
            accounts=[AccountMeta.writable(acc, True)],
            data=data,
        )
        assert ix.program_id == prog
        assert len(ix.accounts) == 1
        assert ix.data == data

    def test_to_json(self):
        prog = key("prog")
        ix = HimshaInstruction(program_id=prog, accounts=[], data=bytes([0xff]))
        j = ix.to_json()
        assert j["programId"] == prog.to_base58()
        assert j["data"] == "ff"


class TestHimshaMessage:
    def test_hash_is_32_bytes(self):
        msg = HimshaMessage(signers=[key("s")], instructions=[], timestamp=0)
        assert len(msg.hash()) == 32

    def test_hash_deterministic(self):
        signer = key("signer")
        prog   = key("prog")
        ix = HimshaInstruction(program_id=prog, accounts=[], data=bytes([1]))
        h1 = HimshaMessage([signer], [ix], timestamp=1000).hash()
        h2 = HimshaMessage([signer], [ix], timestamp=1000).hash()
        assert h1 == h2

    def test_hash_changes_with_timestamp(self):
        signer = key("s")
        h1 = HimshaMessage([signer], [], timestamp=1000).hash()
        h2 = HimshaMessage([signer], [], timestamp=2000).hash()
        assert h1 != h2

    def test_hash_changes_with_signer(self):
        prog = key("prog")
        ix = HimshaInstruction(prog, [], bytes([0]))
        h1 = HimshaMessage([key("alice")], [ix], timestamp=0).hash()
        h2 = HimshaMessage([key("bob")],   [ix], timestamp=0).hash()
        assert h1 != h2

    def test_to_json_structure(self):
        signer = key("s")
        msg = HimshaMessage([signer], [], timestamp=42)
        j = msg.to_json()
        assert j["signers"] == [signer.to_base58()]
        assert j["timestamp"] == "42"
        assert j["instructions"] == []


class TestHimshaTransaction:
    def test_version_is_zero(self):
        tx = HimshaTransaction.create([key("s")], [])
        assert tx.version == 0

    def test_add_signature(self):
        tx  = HimshaTransaction.create([key("s")], [])
        sig = bytes([0xab] * 64)
        tx.add_signature(sig)
        assert len(tx.signatures) == 1

    def test_add_signature_wrong_length(self):
        tx = HimshaTransaction.create([key("s")], [])
        with pytest.raises(ValueError, match="64 bytes"):
            tx.add_signature(bytes(63))
        with pytest.raises(ValueError, match="64 bytes"):
            tx.add_signature(bytes(65))

    def test_message_hash_32_bytes(self):
        tx = HimshaTransaction.create([key("s")], [])
        assert len(tx.message_hash()) == 32

    def test_to_json(self):
        tx  = HimshaTransaction.create([key("s")], [], timestamp=999)
        sig = bytes([0xff] * 64)
        tx.add_signature(sig)
        j = tx.to_json()
        assert j["version"] == 0
        assert len(j["signatures"]) == 1
        assert j["signatures"][0] == "ff" * 64
        assert "message" in j

    def test_chaining(self):
        tx = HimshaTransaction.create([key("s")], [])
        result = tx.add_signature(bytes(64))
        assert result is tx  # returns self
