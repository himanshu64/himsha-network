"""
HIMSHA Network Python SDK
======================
ZK-proven Bitcoin programmability layer.

Quick start:
    from himsha_sdk import HimshaConnection, HimshaPublicKey
    from himsha_sdk.programs import system, token

    conn = HimshaConnection("http://localhost:9100")
    ready = await conn.is_node_ready()
"""

from .pubkey import HimshaPublicKey, PROGRAM_IDS
from .transaction import HimshaInstruction, HimshaMessage, HimshaTransaction, AccountMeta
from .connection import (
    HimshaConnection,
    AccountInfo,
    UtxoInfo,
    HimshaRpcError,
    HimshaTransactionFailed,
)
from .state_proof import (
    leaf_hash,
    node_hash,
    verify_state_proof,
    verify_account_in_state,
)

__all__ = [
    "HimshaPublicKey",
    "PROGRAM_IDS",
    "HimshaInstruction",
    "HimshaMessage",
    "HimshaTransaction",
    "AccountMeta",
    "HimshaConnection",
    "AccountInfo",
    "UtxoInfo",
    "HimshaRpcError",
    "HimshaTransactionFailed",
    "leaf_hash",
    "node_hash",
    "verify_state_proof",
    "verify_account_in_state",
]
