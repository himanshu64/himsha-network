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

    async def confirm_transaction(
        self, tx_id: str, timeout_secs: float = 30.0
    ) -> int:
        """Poll until a transaction is included in a block. Returns slot number."""
        deadline = asyncio.get_event_loop().time() + timeout_secs
        while asyncio.get_event_loop().time() < deadline:
            slot = await self.get_slot()
            block = await self.get_block(slot)
            if block:
                for tx in block.get("transactions", []):
                    if tx.get("id") == tx_id:
                        return slot
            await asyncio.sleep(1.0)
        raise TimeoutError(f"Transaction {tx_id} not confirmed within {timeout_secs}s")

    async def close(self) -> None:
        await self._client.aclose()
