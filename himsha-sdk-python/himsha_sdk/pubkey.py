"""HIMSHA Network public key — 32-byte identifier for accounts and programs."""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from typing import Sequence

import base58  # pip install base58


@dataclass(frozen=True)
class HimshaPublicKey:
    """Immutable 32-byte public key."""

    _bytes: bytes

    def __init__(self, data: bytes | str) -> None:
        if isinstance(data, str):
            decoded = base58.b58decode(data)
        else:
            decoded = bytes(data)
        if len(decoded) != 32:
            raise ValueError(f"PublicKey must be 32 bytes, got {len(decoded)}")
        object.__setattr__(self, "_bytes", decoded)

    # ---- factories ----

    @classmethod
    def from_base58(cls, s: str) -> "HimshaPublicKey":
        return cls(s)

    @classmethod
    def from_seed(cls, seed: str) -> "HimshaPublicKey":
        """Deterministic key from a UTF-8 seed string."""
        digest = hashlib.sha256(seed.encode()).digest()
        return cls(digest)

    @classmethod
    def find_program_address(
        cls, seeds: Sequence[bytes], program_id: "HimshaPublicKey"
    ) -> tuple["HimshaPublicKey", int]:
        """Derive a Program Derived Address (PDA)."""
        for nonce in range(255, -1, -1):
            h = hashlib.sha256()
            for s in seeds:
                h.update(s)
            h.update(bytes([nonce]))
            h.update(program_id.to_bytes())
            h.update(b"himsha::pda")
            return cls(h.digest()), nonce
        raise RuntimeError("Could not find program address")

    # ---- accessors ----

    def to_bytes(self) -> bytes:
        return self._bytes

    def to_base58(self) -> str:
        return base58.b58encode(self._bytes).decode()

    def __str__(self) -> str:
        return self.to_base58()

    def __repr__(self) -> str:
        return f"HimshaPublicKey({self.to_base58()[:8]}…)"

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, HimshaPublicKey):
            return NotImplemented
        return self._bytes == other._bytes

    def __hash__(self) -> int:
        return hash(self._bytes)


class ProgramIds:
    """Well-known program IDs — derived deterministically from seeds."""

    system       = HimshaPublicKey.from_seed("himsha::system_program")
    token        = HimshaPublicKey.from_seed("himsha::token_program")
    ata          = HimshaPublicKey.from_seed("himsha::ata_program")
    swap         = HimshaPublicKey.from_seed("himsha::swap_program")
    lending      = HimshaPublicKey.from_seed("himsha::lending_program")
    nft_metadata = HimshaPublicKey.from_seed("himsha::nft_metadata_program")


#: Shorthand alias
PROGRAM_IDS = ProgramIds()
