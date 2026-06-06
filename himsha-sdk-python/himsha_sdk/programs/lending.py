"""HIMSHA Lending Program helpers."""

import struct
from ..pubkey import HimshaPublicKey, PROGRAM_IDS
from ..transaction import AccountMeta, HimshaInstruction


def _u64(n: int) -> bytes:
    return struct.pack("<Q", n)


def _u32(n: int) -> bytes:
    return struct.pack("<I", n)


def _str(s: str) -> bytes:
    encoded = s.encode()
    return _u32(len(encoded)) + encoded


def init_collection(
    collection: HimshaPublicKey, payer: HimshaPublicKey, name: str
) -> HimshaInstruction:
    return HimshaInstruction(
        program_id=PROGRAM_IDS.lending,
        accounts=[
            AccountMeta.writable(collection, False),
            AccountMeta.writable(payer, True),
        ],
        data=bytes([0]) + _str(name),
    )


def place_bid(
    collection: HimshaPublicKey,
    lender: HimshaPublicKey,
    bid_txid: bytes,        # 32 bytes
    bid_vout: int,
    loan_value_sats: int,
    loan_period_secs: int,
    lender_ordinals_addr: str,
    lender_payments_addr: str,
) -> HimshaInstruction:
    if len(bid_txid) != 32:
        raise ValueError("bid_txid must be 32 bytes")
    return HimshaInstruction(
        program_id=PROGRAM_IDS.lending,
        accounts=[
            AccountMeta.writable(collection, False),
            AccountMeta.readonly(lender, True),
        ],
        data=(
            bytes([1]) + bid_txid + _u32(bid_vout)
            + _u64(loan_value_sats) + _u64(loan_period_secs)
            + _str(lender_ordinals_addr) + _str(lender_payments_addr)
        ),
    )


def accept_bid(
    collection: HimshaPublicKey,
    borrower: HimshaPublicKey,
    inscription_id: str,
    inscription_txid: bytes,   # 32 bytes
    inscription_vout: int,
    borrower_ordinals_addr: str,
    borrower_payments_addr: str,
) -> HimshaInstruction:
    if len(inscription_txid) != 32:
        raise ValueError("inscription_txid must be 32 bytes")
    return HimshaInstruction(
        program_id=PROGRAM_IDS.lending,
        accounts=[
            AccountMeta.writable(collection, False),
            AccountMeta.readonly(borrower, True),
        ],
        data=(
            bytes([2]) + _str(inscription_id)
            + inscription_txid + _u32(inscription_vout)
            + _str(borrower_ordinals_addr) + _str(borrower_payments_addr)
        ),
    )


def repay_loan(
    collection: HimshaPublicKey,
    borrower: HimshaPublicKey,
    inscription_id: str,
    repay_txid: bytes,    # 32 bytes
    repay_vout: int,
) -> HimshaInstruction:
    if len(repay_txid) != 32:
        raise ValueError("repay_txid must be 32 bytes")
    return HimshaInstruction(
        program_id=PROGRAM_IDS.lending,
        accounts=[
            AccountMeta.writable(collection, False),
            AccountMeta.readonly(borrower, True),
        ],
        data=bytes([3]) + _str(inscription_id) + repay_txid + _u32(repay_vout),
    )


def claim_default(
    collection: HimshaPublicKey, lender: HimshaPublicKey, inscription_id: str
) -> HimshaInstruction:
    return HimshaInstruction(
        program_id=PROGRAM_IDS.lending,
        accounts=[
            AccountMeta.writable(collection, False),
            AccountMeta.readonly(lender, True),
        ],
        data=bytes([4]) + _str(inscription_id),
    )
