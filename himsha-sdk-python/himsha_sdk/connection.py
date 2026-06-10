"""Async JSON-RPC connection to a HIMSHA Network node."""

from __future__ import annotations

import asyncio
from dataclasses import dataclass
from typing import Any, TypeVar

import httpx

from .pubkey import HimshaPublicKey
from .transaction import HimshaTransaction

T = TypeVar("T")


class HimshaRpcError(Exception):
    def __init__(self, code: int, message: str) -> None:
        super().__init__(f"RPC error {code}: {message}")
        self.code = code


class HimshaTransactionFailed(Exception):
    """Raised by :meth:`HimshaConnection.confirm_transaction` when a transaction
    executed but its on-chain status is ``failed``."""


@dataclass
class AccountInfo:
    key:        str
    lamports:   int
    data_hex:   str
    owner:      str
    executable: bool

    @property
    def data(self) -> bytes:
        return bytes.fromhex(self.data_hex)

    @classmethod
    def from_json(cls, j: dict) -> "AccountInfo":
        return cls(
            key=j["key"],
            lamports=int(j["lamports"]),
            data_hex=j["data"],
            owner=j["owner"],
            executable=j["executable"],
        )


@dataclass
class UtxoInfo:
    txid:          str
    vout:          int
    value:         int   # satoshis
    script_pubkey: str
    confirmations: int

    @classmethod
    def from_json(cls, j: dict) -> "UtxoInfo":
        meta = j["meta"]
        return cls(
            txid=meta["txid"],
            vout=meta["vout"],
            value=int(j["value"]),
            script_pubkey=j["scriptPubkey"],
            confirmations=j["confirmations"],
        )


class HimshaConnection:
    """
    Async JSON-RPC client for HIMSHA Network.

    Usage::

        async with HimshaConnection("http://localhost:9100") as conn:
            ready = await conn.is_node_ready()
            slot  = await conn.get_slot()
    """

    def __init__(self, endpoint: str, timeout: float = 30.0) -> None:
        self.endpoint = endpoint
        self._client  = httpx.AsyncClient(timeout=timeout)
        self._next_id = 1

    async def __aenter__(self) -> "HimshaConnection":
        return self

    async def __aexit__(self, *_: Any) -> None:
        await self.close()

    # ---- low-level ----

    async def _call(self, method: str, params: list[Any] | None = None) -> Any:
        req_id = self._next_id
        self._next_id += 1
        body = {"jsonrpc": "2.0", "id": req_id, "method": method, "params": params or []}
        response = await self._client.post(
            self.endpoint,
            json=body,
            headers={"Content-Type": "application/json"},
        )
        response.raise_for_status()
        data = response.json()
        if "error" in data:
            err = data["error"]
            raise HimshaRpcError(err["code"], err["message"])
        return data["result"]

    # ---- node ----

    async def is_node_ready(self) -> bool:
        return await self._call("himsha_isNodeReady")

    async def get_slot(self) -> int:
        slot = await self._call("himsha_getSlot")
        return int(slot)

    async def get_block(self, slot: int) -> dict | None:
        return await self._call("himsha_getBlock", [str(slot)])

    async def list_programs(self) -> list[str]:
        return await self._call("himsha_listPrograms")

    # ---- accounts ----

    async def get_account_info(self, pubkey: HimshaPublicKey | str) -> AccountInfo | None:
        key = str(pubkey)
        raw = await self._call("himsha_getAccountInfo", [key])
        return AccountInfo.from_json(raw) if raw else None

    async def get_program_accounts(self, program_id: HimshaPublicKey | str) -> list[AccountInfo]:
        pid = str(program_id)
        raw_list = await self._call("himsha_getProgramAccounts", [pid])
        return [AccountInfo.from_json(r) for r in (raw_list or [])]

    async def account_exists(self, pubkey: HimshaPublicKey | str) -> bool:
        return (await self.get_account_info(pubkey)) is not None

    # ---- bitcoin ----

    async def get_utxo(self, txid: str, vout: int) -> UtxoInfo | None:
        raw = await self._call("himsha_getUtxo", [txid, vout])
        return UtxoInfo.from_json(raw) if raw else None

    # ---- transactions ----

    async def send_transaction(self, tx: HimshaTransaction) -> str:
        """Submit a signed transaction. Returns the transaction ID (hex)."""
        return await self._call("himsha_sendTransaction", [tx.to_json()])

    async def deploy_program(self, elf_hex: str, image_id_hex: str) -> str:
        """Deploy a compiled ELF. Returns the program's public key (base58)."""
        return await self._call("himsha_deployProgram", [elf_hex, image_id_hex])

    async def get_signature_status(self, txid: str) -> dict | None:
        """
        Execution status of a submitted transaction, or ``None`` if the node has
        never seen the id. Since execution happens at block production (not at
        submit time), this is how a client learns the authoritative outcome.

        Returns a dict ``{status, slot, error}`` where ``status`` is one of
        ``"pending"``, ``"succeeded"``, ``"failed"``.
        """
        return await self._call("himsha_getSignatureStatus", [txid])

    async def get_state_proof(self, pubkey: HimshaPublicKey | str) -> dict | None:
        """
        State-root inclusion proof for an account, or ``None`` if not found.

        Returns a dict ``{state_root, leaf, index, siblings, anchored_slot,
        anchored_state_root, anchored_btc_txid}``. Verify it with
        ``himsha_sdk.state_proof.verify_state_proof`` /
        ``verify_account_in_state``.
        """
        key = str(pubkey)
        return await self._call("himsha_getStateProof", [key])

    async def get_custody_info(self) -> dict | None:
        """
        Threshold-custody settlement configuration, or ``None`` if custody is not
        configured. Returns a dict ``{threshold, total, group_key, address}``.
        """
        return await self._call("himsha_getCustodyInfo")

    async def confirm_transaction(
        self, tx_id: str, timeout_secs: float = 30.0
    ) -> int:
        """
        Poll until a transaction is executed. Returns the slot once it
        ``succeeded``; **raises** with the failure reason if it ``failed`` (no
        silent timeouts on a rejected tx); keeps waiting while ``pending``;
        raises :class:`TimeoutError` if it never settles in time.
        """
        deadline = asyncio.get_event_loop().time() + timeout_secs
        while asyncio.get_event_loop().time() < deadline:
            st = await self.get_signature_status(tx_id)
            if st is not None:
                status = st.get("status")
                if status == "succeeded":
                    return int(st.get("slot") or 0)
                if status == "failed":
                    slot = st.get("slot")
                    at = f" at slot {slot}" if slot is not None else ""
                    reason = st.get("error") or "unknown error"
                    raise HimshaTransactionFailed(
                        f"Transaction {tx_id} failed{at}: {reason}"
                    )
            await asyncio.sleep(0.5)
        raise TimeoutError(f"Transaction {tx_id} not confirmed within {timeout_secs}s")

    async def close(self) -> None:
        await self._client.aclose()
