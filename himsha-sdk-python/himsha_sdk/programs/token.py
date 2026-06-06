"""HIMSHA Token Program helpers."""

import struct
from ..pubkey import HimshaPublicKey, PROGRAM_IDS
from ..transaction import AccountMeta, HimshaInstruction


def _u64(n: int) -> bytes:
    return struct.pack("<Q", n)


def _opt_key(key: HimshaPublicKey | None) -> bytes:
    return (b"\x01" + key.to_bytes()) if key else b"\x00"


def initialize_mint(
    mint: HimshaPublicKey,
    authority: HimshaPublicKey,
    decimals: int,
    freeze_authority: HimshaPublicKey | None = None,
) -> HimshaInstruction:
    data = bytes([0, decimals]) + authority.to_bytes() + _opt_key(freeze_authority)
    return HimshaInstruction(
        program_id=PROGRAM_IDS.token,
        accounts=[AccountMeta.writable(mint, is_signer=False)],
        data=data,
    )


def initialize_account(
    account: HimshaPublicKey, mint: HimshaPublicKey, owner: HimshaPublicKey
) -> HimshaInstruction:
    return HimshaInstruction(
        program_id=PROGRAM_IDS.token,
        accounts=[
            AccountMeta.writable(account, is_signer=False),
            AccountMeta.readonly(mint, is_signer=False),
            AccountMeta.readonly(owner, is_signer=False),
        ],
        data=bytes([1]),
    )


def mint_to(
    mint: HimshaPublicKey,
    destination: HimshaPublicKey,
    authority: HimshaPublicKey,
    amount: int,
) -> HimshaInstruction:
    return HimshaInstruction(
        program_id=PROGRAM_IDS.token,
        accounts=[
            AccountMeta.writable(mint, is_signer=False),
            AccountMeta.writable(destination, is_signer=False),
            AccountMeta.readonly(authority, is_signer=True),
        ],
        data=bytes([2]) + _u64(amount),
    )


def transfer(
    source: HimshaPublicKey, destination: HimshaPublicKey, owner: HimshaPublicKey, amount: int
) -> HimshaInstruction:
    return HimshaInstruction(
        program_id=PROGRAM_IDS.token,
        accounts=[
            AccountMeta.writable(source, is_signer=False),
            AccountMeta.writable(destination, is_signer=False),
            AccountMeta.readonly(owner, is_signer=True),
        ],
        data=bytes([3]) + _u64(amount),
    )


def burn(
    account: HimshaPublicKey, mint: HimshaPublicKey, owner: HimshaPublicKey, amount: int
) -> HimshaInstruction:
    return HimshaInstruction(
        program_id=PROGRAM_IDS.token,
        accounts=[
            AccountMeta.writable(account, is_signer=False),
            AccountMeta.writable(mint, is_signer=False),
            AccountMeta.readonly(owner, is_signer=True),
        ],
        data=bytes([4]) + _u64(amount),
    )


def close_account(
    account: HimshaPublicKey, destination: HimshaPublicKey, owner: HimshaPublicKey
) -> HimshaInstruction:
    return HimshaInstruction(
        program_id=PROGRAM_IDS.token,
        accounts=[
            AccountMeta.writable(account, is_signer=False),
            AccountMeta.writable(destination, is_signer=False),
            AccountMeta.readonly(owner, is_signer=True),
        ],
        data=bytes([9]),
    )
