"""HIMSHA transaction types: AccountMeta, Instruction, Message, Transaction."""

from __future__ import annotations

import hashlib
import struct
import time
from dataclasses import dataclass, field
from typing import Sequence

from .pubkey import HimshaPublicKey


@dataclass
class AccountMeta:
    pubkey: HimshaPublicKey
    is_signer: bool
    is_writable: bool

    @classmethod
    def writable(cls, pubkey: HimshaPublicKey, is_signer: bool) -> "AccountMeta":
        return cls(pubkey=pubkey, is_signer=is_signer, is_writable=True)

    @classmethod
    def readonly(cls, pubkey: HimshaPublicKey, is_signer: bool) -> "AccountMeta":
        return cls(pubkey=pubkey, is_signer=is_signer, is_writable=False)

    def to_json(self) -> dict:
        return {
            "pubkey":     self.pubkey.to_base58(),
            "isSigner":   self.is_signer,
            "isWritable": self.is_writable,
        }


@dataclass
class HimshaInstruction:
    program_id: HimshaPublicKey
    accounts:   list[AccountMeta]
    data:       bytes

    def to_json(self) -> dict:
        return {
            "programId": self.program_id.to_base58(),
            "accounts":  [a.to_json() for a in self.accounts],
            "data":      self.data.hex(),
        }


@dataclass
class HimshaMessage:
    signers:      list[HimshaPublicKey]
    instructions: list[HimshaInstruction]
    timestamp:    int = field(default_factory=lambda: int(time.time()))

    def hash(self) -> bytes:
        """SHA-256 of the message — this is what signers sign."""
        h = hashlib.sha256()
        for s in self.signers:
            h.update(s.to_bytes())
        for instr in self.instructions:
            h.update(instr.program_id.to_bytes())
            for acc in instr.accounts:
                h.update(acc.pubkey.to_bytes())
                h.update(bytes([1 if acc.is_signer else 0, 1 if acc.is_writable else 0]))
            h.update(instr.data)
        h.update(struct.pack("<Q", self.timestamp))
        return h.digest()

    def to_json(self) -> dict:
        return {
            "signers":      [s.to_base58() for s in self.signers],
            "instructions": [i.to_json() for i in self.instructions],
            "timestamp":    str(self.timestamp),
        }


class HimshaTransaction:
    version = 0

    def __init__(self, message: HimshaMessage) -> None:
        self.message    = message
        self.signatures: list[bytes] = []

    @classmethod
    def create(
        cls,
        signers:      Sequence[HimshaPublicKey],
        instructions: Sequence[HimshaInstruction],
        timestamp:    int | None = None,
    ) -> "HimshaTransaction":
        msg = HimshaMessage(
            signers=list(signers),
            instructions=list(instructions),
            timestamp=timestamp if timestamp is not None else int(time.time()),
        )
        return cls(msg)

    def add_signature(self, sig: bytes) -> "HimshaTransaction":
        if len(sig) != 64:
            raise ValueError(f"Signature must be 64 bytes, got {len(sig)}")
        self.signatures.append(sig)
        return self

    def message_hash(self) -> bytes:
        return self.message.hash()

    def to_json(self) -> dict:
        return {
            "version":    self.version,
            "signatures": [s.hex() for s in self.signatures],
            "message":    self.message.to_json(),
        }
