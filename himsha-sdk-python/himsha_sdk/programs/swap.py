"""HIMSHA Swap Program helpers."""

import struct
from ..pubkey import HimshaPublicKey, PROGRAM_IDS
from ..transaction import AccountMeta, HimshaInstruction


def _u64(n: int) -> bytes:
    return struct.pack("<Q", n)


def initialize(
    pool: HimshaPublicKey, mint_a: HimshaPublicKey, mint_b: HimshaPublicKey,
    reserve_a: HimshaPublicKey, reserve_b: HimshaPublicKey,
    lp_mint: HimshaPublicKey, payer: HimshaPublicKey,
    fee_numerator: int, fee_denominator: int,
) -> HimshaInstruction:
    return HimshaInstruction(
        program_id=PROGRAM_IDS.swap,
        accounts=[
            AccountMeta.writable(pool, False),
            AccountMeta.readonly(mint_a, False), AccountMeta.readonly(mint_b, False),
            AccountMeta.writable(reserve_a, False), AccountMeta.writable(reserve_b, False),
            AccountMeta.writable(lp_mint, False), AccountMeta.writable(payer, True),
        ],
        data=bytes([0]) + _u64(fee_numerator) + _u64(fee_denominator),
    )


def swap(
    pool: HimshaPublicKey, source: HimshaPublicKey, destination: HimshaPublicKey,
    reserve_in: HimshaPublicKey, reserve_out: HimshaPublicKey, user: HimshaPublicKey,
    amount_in: int, min_amount_out: int,
) -> HimshaInstruction:
    return HimshaInstruction(
        program_id=PROGRAM_IDS.swap,
        accounts=[
            AccountMeta.readonly(pool, False),
            AccountMeta.writable(source, False), AccountMeta.writable(destination, False),
            AccountMeta.writable(reserve_in, False), AccountMeta.writable(reserve_out, False),
            AccountMeta.readonly(user, True),
        ],
        data=bytes([1]) + _u64(amount_in) + _u64(min_amount_out),
    )


def deposit(
    pool: HimshaPublicKey, user_a: HimshaPublicKey, user_b: HimshaPublicKey,
    reserve_a: HimshaPublicKey, reserve_b: HimshaPublicKey,
    user_lp: HimshaPublicKey, user: HimshaPublicKey,
    max_a: int, max_b: int, min_lp: int,
) -> HimshaInstruction:
    return HimshaInstruction(
        program_id=PROGRAM_IDS.swap,
        accounts=[
            AccountMeta.writable(pool, False),
            AccountMeta.writable(user_a, False), AccountMeta.writable(user_b, False),
            AccountMeta.writable(reserve_a, False), AccountMeta.writable(reserve_b, False),
            AccountMeta.writable(user_lp, False), AccountMeta.readonly(user, True),
        ],
        data=bytes([2]) + _u64(max_a) + _u64(max_b) + _u64(min_lp),
    )


def withdraw(
    pool: HimshaPublicKey, user_a: HimshaPublicKey, user_b: HimshaPublicKey,
    reserve_a: HimshaPublicKey, reserve_b: HimshaPublicKey,
    user_lp: HimshaPublicKey, user: HimshaPublicKey,
    lp_amount: int, min_a: int, min_b: int,
) -> HimshaInstruction:
    return HimshaInstruction(
        program_id=PROGRAM_IDS.swap,
        accounts=[
            AccountMeta.writable(pool, False),
            AccountMeta.writable(user_a, False), AccountMeta.writable(user_b, False),
            AccountMeta.writable(reserve_a, False), AccountMeta.writable(reserve_b, False),
            AccountMeta.writable(user_lp, False), AccountMeta.readonly(user, True),
        ],
        data=bytes([3]) + _u64(lp_amount) + _u64(min_a) + _u64(min_b),
    )
