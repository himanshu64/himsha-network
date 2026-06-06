"""HIMSHA System Program helpers."""

import struct
from ..pubkey import HimshaPublicKey, PROGRAM_IDS
from ..transaction import AccountMeta, HimshaInstruction


def _u64(n: int) -> bytes:
    return struct.pack("<Q", n)


def create_account(
    payer: HimshaPublicKey,
    new_account: HimshaPublicKey,
    lamports: int,
    space: int,
    owner: HimshaPublicKey,
) -> HimshaInstruction:
    data = bytes([0]) + _u64(lamports) + _u64(space) + owner.to_bytes()
    return HimshaInstruction(
        program_id=PROGRAM_IDS.system,
        accounts=[
            AccountMeta.writable(payer, is_signer=True),
            AccountMeta.writable(new_account, is_signer=True),
        ],
        data=data,
    )


def transfer(from_: HimshaPublicKey, to: HimshaPublicKey, lamports: int) -> HimshaInstruction:
    return HimshaInstruction(
        program_id=PROGRAM_IDS.system,
        accounts=[
            AccountMeta.writable(from_, is_signer=True),
            AccountMeta.writable(to, is_signer=False),
        ],
        data=bytes([2]) + _u64(lamports),
    )


def assign(account: HimshaPublicKey, owner: HimshaPublicKey) -> HimshaInstruction:
    return HimshaInstruction(
        program_id=PROGRAM_IDS.system,
        accounts=[AccountMeta.writable(account, is_signer=True)],
        data=bytes([3]) + owner.to_bytes(),
    )


def allocate(account: HimshaPublicKey, space: int) -> HimshaInstruction:
    return HimshaInstruction(
        program_id=PROGRAM_IDS.system,
        accounts=[AccountMeta.writable(account, is_signer=True)],
        data=bytes([4]) + _u64(space),
    )
