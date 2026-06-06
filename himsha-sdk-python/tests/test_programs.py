"""Tests for all program instruction builders."""

import struct
import pytest
from himsha_sdk.pubkey import HimshaPublicKey, PROGRAM_IDS
from himsha_sdk.programs import system, token, swap, lending


def key(seed: str) -> HimshaPublicKey:
    return HimshaPublicKey.from_seed(seed)


# ============================================================
# System Program
# ============================================================
class TestSystemProgram:
    payer   = key("payer")
    new_acc = key("newAccount")
    owner   = key("owner")

    def test_create_account_program_id(self):
        ix = system.create_account(self.payer, self.new_acc, 1_000_000, 128, self.owner)
        assert ix.program_id == PROGRAM_IDS.system

    def test_create_account_discriminant(self):
        ix = system.create_account(self.payer, self.new_acc, 100, 64, self.owner)
        assert ix.data[0] == 0

    def test_create_account_encodes_lamports(self):
        lamports = 999_999
        ix = system.create_account(self.payer, self.new_acc, lamports, 64, self.owner)
        decoded = struct.unpack_from("<Q", ix.data, 1)[0]
        assert decoded == lamports

    def test_create_account_has_2_accounts(self):
        ix = system.create_account(self.payer, self.new_acc, 100, 64, self.owner)
        assert len(ix.accounts) == 2
        assert ix.accounts[0].is_signer is True
        assert ix.accounts[0].is_writable is True

    def test_transfer_discriminant(self):
        ix = system.transfer(self.payer, self.new_acc, 500)
        assert ix.data[0] == 2
        assert ix.accounts[0].is_signer is True
        assert ix.accounts[1].is_signer is False

    def test_assign_discriminant(self):
        ix = system.assign(self.new_acc, self.owner)
        assert ix.data[0] == 3

    def test_allocate_discriminant(self):
        ix = system.allocate(self.new_acc, 256)
        assert ix.data[0] == 4


# ============================================================
# Token Program
# ============================================================
class TestTokenProgram:
    mint      = key("mint")
    authority = key("authority")
    account   = key("tokenAccount")
    owner     = key("owner")
    dest      = key("destination")

    def test_initialize_mint_program_id(self):
        ix = token.initialize_mint(self.mint, self.authority, 6)
        assert ix.program_id == PROGRAM_IDS.token

    def test_initialize_mint_discriminant_and_decimals(self):
        ix = token.initialize_mint(self.mint, self.authority, 8)
        assert ix.data[0] == 0
        assert ix.data[1] == 8  # decimals

    def test_initialize_mint_no_freeze(self):
        ix = token.initialize_mint(self.mint, self.authority, 6)
        assert ix.data[34] == 0  # no freeze authority flag

    def test_initialize_mint_with_freeze(self):
        freeze = key("freeze")
        ix = token.initialize_mint(self.mint, self.authority, 6, freeze)
        assert ix.data[34] == 1  # freeze authority present

    def test_mint_to_discriminant(self):
        ix = token.mint_to(self.mint, self.dest, self.authority, 1_000_000)
        assert ix.data[0] == 2
        assert ix.accounts[2].is_signer is True

    def test_transfer_discriminant(self):
        ix = token.transfer(self.account, self.dest, self.owner, 500)
        assert ix.data[0] == 3

    def test_burn_discriminant(self):
        ix = token.burn(self.account, self.mint, self.owner, 100)
        assert ix.data[0] == 4

    def test_close_account_discriminant(self):
        ix = token.close_account(self.account, self.dest, self.owner)
        assert ix.data[0] == 9


# ============================================================
# Swap Program
# ============================================================
class TestSwapProgram:
    k = staticmethod(key)

    def test_initialize_discriminant_and_accounts(self):
        ix = swap.initialize(
            self.k("pool"), self.k("mA"), self.k("mB"),
            self.k("rA"), self.k("rB"), self.k("lp"), self.k("payer"),
            3, 1000,
        )
        assert ix.data[0] == 0
        assert len(ix.accounts) == 7

    def test_initialize_encodes_fees(self):
        ix = swap.initialize(
            self.k("pool"), self.k("mA"), self.k("mB"),
            self.k("rA"), self.k("rB"), self.k("lp"), self.k("payer"),
            3, 1000,
        )
        fee_num = struct.unpack_from("<Q", ix.data, 1)[0]
        fee_den = struct.unpack_from("<Q", ix.data, 9)[0]
        assert fee_num == 3
        assert fee_den == 1000

    def test_swap_discriminant(self):
        ix = swap.swap(
            self.k("pool"), self.k("src"), self.k("dst"),
            self.k("rIn"), self.k("rOut"), self.k("user"),
            100, 90,
        )
        assert ix.data[0] == 1
        assert len(ix.accounts) == 6

    def test_deposit_discriminant(self):
        ix = swap.deposit(
            self.k("p"), self.k("uA"), self.k("uB"),
            self.k("rA"), self.k("rB"), self.k("lp"), self.k("u"),
            1000, 1000, 1,
        )
        assert ix.data[0] == 2

    def test_withdraw_discriminant(self):
        ix = swap.withdraw(
            self.k("p"), self.k("uA"), self.k("uB"),
            self.k("rA"), self.k("rB"), self.k("lp"), self.k("u"),
            500, 450, 450,
        )
        assert ix.data[0] == 3


# ============================================================
# Lending Program
# ============================================================
class TestLendingProgram:
    collection = key("collection")
    payer      = key("payer")
    lender     = key("lender")
    borrower   = key("borrower")
    txid       = bytes([0xcc] * 32)

    def test_init_collection_discriminant(self):
        ix = lending.init_collection(self.collection, self.payer, "FrogNFTs")
        assert ix.data[0] == 0
        assert ix.program_id == PROGRAM_IDS.lending

    def test_init_collection_encodes_name(self):
        name = "TestCollection"
        ix = lending.init_collection(self.collection, self.payer, name)
        name_len = struct.unpack_from("<I", ix.data, 1)[0]
        assert name_len == len(name)
        assert ix.data[5 : 5 + len(name)] == name.encode()

    def test_place_bid_discriminant_and_lender_signs(self):
        ix = lending.place_bid(
            self.collection, self.lender,
            self.txid, 0, 100_000, 2_592_000,
            "tb1q_lender_ord", "tb1q_lender_pay",
        )
        assert ix.data[0] == 1
        assert ix.accounts[1].is_signer is True

    def test_place_bid_txid_in_data(self):
        ix = lending.place_bid(
            self.collection, self.lender,
            self.txid, 0, 50_000, 86_400,
            "addr1", "addr2",
        )
        # txid starts at byte 1
        assert ix.data[1:33] == self.txid

    def test_place_bid_wrong_txid_length(self):
        with pytest.raises(ValueError, match="32 bytes"):
            lending.place_bid(self.collection, self.lender, bytes(31), 0, 1, 1, "", "")

    def test_accept_bid_discriminant(self):
        ix = lending.accept_bid(
            self.collection, self.borrower,
            "abc123i0", self.txid, 0,
            "tb1q_borrower_ord", "tb1q_borrower_pay",
        )
        assert ix.data[0] == 2

    def test_repay_loan_discriminant(self):
        ix = lending.repay_loan(
            self.collection, self.borrower,
            "abc123i0", bytes([0xdd] * 32), 0,
        )
        assert ix.data[0] == 3

    def test_claim_default_discriminant_and_lender_signs(self):
        ix = lending.claim_default(self.collection, self.lender, "abc123i0")
        assert ix.data[0] == 4
        assert ix.accounts[1].is_signer is True

    def test_claim_default_wrong_txid_in_repay(self):
        with pytest.raises(ValueError, match="32 bytes"):
            lending.repay_loan(
                self.collection, self.borrower,
                "id", bytes(31), 0,
            )
