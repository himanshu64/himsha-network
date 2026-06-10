import 'dart:typed_data';
import 'package:test/test.dart';
import 'package:himsha_sdk/himsha_sdk.dart';

void main() {
  // ---- HimshaPublicKey ----
  group('HimshaPublicKey', () {
    test('creates from 32 bytes', () {
      final bytes = Uint8List(32)..fillRange(0, 32, 1);
      final pk = HimshaPublicKey(bytes);
      expect(pk.bytes, equals(bytes));
    });

    test('throws on wrong byte length', () {
      expect(() => HimshaPublicKey(Uint8List(31)), throwsArgumentError);
      expect(() => HimshaPublicKey(Uint8List(33)), throwsArgumentError);
    });

    test('round-trips through base58', () {
      final bytes = Uint8List(32)..fillRange(0, 32, 42);
      final pk = HimshaPublicKey(bytes);
      final restored = HimshaPublicKey.fromBase58(pk.toBase58());
      expect(pk, equals(restored));
    });

    test('fromSeed is deterministic', () {
      final a = HimshaPublicKey.fromSeed('test');
      final b = HimshaPublicKey.fromSeed('test');
      final c = HimshaPublicKey.fromSeed('other');
      expect(a, equals(b));
      expect(a, isNot(equals(c)));
    });

    test('ProgramIds are correct', () {
      expect(
        ProgramIds.system,
        equals(HimshaPublicKey.fromSeed('himsha::system_program')),
      );
      expect(
        ProgramIds.token,
        equals(HimshaPublicKey.fromSeed('himsha::token_program')),
      );
      expect(
        ProgramIds.lending,
        equals(HimshaPublicKey.fromSeed('himsha::lending_program')),
      );
    });

    test('toString returns base58', () {
      final pk = HimshaPublicKey.fromSeed('hello');
      expect(pk.toString(), equals(pk.toBase58()));
    });
  });

  // ---- HimshaTransaction ----
  group('HimshaTransaction', () {
    test('version is 0', () {
      final tx = HimshaTransaction.create([], []);
      expect(tx.version, equals(0));
    });

    test('addSignature accepts 64-byte sig', () {
      final tx = HimshaTransaction.create([], []);
      final sig = Uint8List(64)..fillRange(0, 64, 0xab);
      tx.addSignature(sig);
      expect(tx.signatures, hasLength(1));
    });

    test('addSignature rejects wrong length', () {
      final tx = HimshaTransaction.create([], []);
      expect(() => tx.addSignature(Uint8List(63)), throwsArgumentError);
      expect(() => tx.addSignature(Uint8List(65)), throwsArgumentError);
    });

    test('messageHash is 32 bytes', () {
      final tx = HimshaTransaction.create([], []);
      expect(tx.messageHash(), hasLength(32));
    });

    test('toJson has correct structure', () {
      final signer = HimshaPublicKey.fromSeed('signer');
      final tx = HimshaTransaction.create([signer], []);
      final sig = Uint8List(64)..fillRange(0, 64, 0xff);
      tx.addSignature(sig);
      final json = tx.toJson();

      expect(json['version'], equals(0));
      expect((json['signatures'] as List), hasLength(1));
      expect((json['message'] as Map)['signers'], hasLength(1));
    });
  });

  // ---- AccountMeta ----
  group('AccountMeta', () {
    test('writable creates correct meta', () {
      final pk = HimshaPublicKey.fromSeed('pk');
      final meta = AccountMeta.writable(pk, true);
      expect(meta.isSigner, isTrue);
      expect(meta.isWritable, isTrue);
    });

    test('readonly creates correct meta', () {
      final pk = HimshaPublicKey.fromSeed('pk');
      final meta = AccountMeta.readonly(pk, false);
      expect(meta.isWritable, isFalse);
      expect(meta.isSigner, isFalse);
    });
  });

  // ---- SystemProgram ----
  group('SystemProgram', () {
    final payer  = HimshaPublicKey.fromSeed('payer');
    final newAcc = HimshaPublicKey.fromSeed('newAccount');
    final owner  = HimshaPublicKey.fromSeed('owner');

    test('createAccount targets system program', () {
      final ix = SystemProgram.createAccount(payer, newAcc, BigInt.from(1000000), BigInt.from(128), owner);
      expect(ix.programId, equals(ProgramIds.system));
    });

    test('createAccount discriminant is 0', () {
      final ix = SystemProgram.createAccount(payer, newAcc, BigInt.from(1000), BigInt.from(64), owner);
      expect(ix.data[0], equals(0));
    });

    test('transfer discriminant is 2', () {
      final ix = SystemProgram.transfer(payer, newAcc, BigInt.from(500));
      expect(ix.data[0], equals(2));
    });

    test('assign discriminant is 3', () {
      final ix = SystemProgram.assign(newAcc, owner);
      expect(ix.data[0], equals(3));
    });
  });

  // ---- TokenProgram ----
  group('TokenProgram', () {
    final mint      = HimshaPublicKey.fromSeed('mint');
    final authority = HimshaPublicKey.fromSeed('authority');
    final account   = HimshaPublicKey.fromSeed('account');
    final owner     = HimshaPublicKey.fromSeed('owner');
    final dest      = HimshaPublicKey.fromSeed('dest');

    test('initializeMint has discriminant 0 and decimals at byte 1', () {
      final ix = TokenProgram.initializeMint(mint, authority, 6);
      expect(ix.data[0], equals(0));
      expect(ix.data[1], equals(6));
    });

    test('initializeMint no freeze authority sets 0 flag', () {
      final ix = TokenProgram.initializeMint(mint, authority, 8);
      expect(ix.data[34], equals(0));
    });

    test('mintTo has discriminant 2', () {
      final ix = TokenProgram.mintTo(mint, dest, authority, BigInt.from(1000000));
      expect(ix.data[0], equals(2));
      expect(ix.accounts[2].isSigner, isTrue);
    });

    test('transfer has discriminant 3', () {
      final ix = TokenProgram.transfer(account, dest, owner, BigInt.from(500));
      expect(ix.data[0], equals(3));
    });

    test('burn has discriminant 4', () {
      final ix = TokenProgram.burn(account, mint, owner, BigInt.from(100));
      expect(ix.data[0], equals(4));
    });
  });

  // ---- SwapProgram ----
  group('SwapProgram', () {
    final k = (String s) => HimshaPublicKey.fromSeed(s);

    test('initialize has discriminant 0 and 7 accounts', () {
      final ix = SwapProgram.initialize(
        k('pool'), k('mA'), k('mB'), k('rA'), k('rB'), k('lp'), k('payer'),
        BigInt.from(3), BigInt.from(1000),
      );
      expect(ix.data[0], equals(0));
      expect(ix.accounts, hasLength(7));
    });

    test('swap has discriminant 1', () {
      final ix = SwapProgram.swap(
        k('pool'), k('src'), k('dst'), k('rIn'), k('rOut'), k('user'),
        BigInt.from(100), BigInt.from(90),
      );
      expect(ix.data[0], equals(1));
    });

    test('deposit has discriminant 2', () {
      final ix = SwapProgram.deposit(
        k('p'), k('uA'), k('uB'), k('rA'), k('rB'), k('lp'), k('u'),
        BigInt.from(100), BigInt.from(100), BigInt.from(1),
      );
      expect(ix.data[0], equals(2));
    });
  });

  // ---- LendingProgram ----
  group('LendingProgram', () {
    final collection = HimshaPublicKey.fromSeed('collection');
    final payer      = HimshaPublicKey.fromSeed('payer');
    final lender     = HimshaPublicKey.fromSeed('lender');
    final borrower   = HimshaPublicKey.fromSeed('borrower');
    final txid       = Uint8List(32)..fillRange(0, 32, 0xcc);

    test('initCollection has discriminant 0', () {
      final ix = LendingProgram.initCollection(collection, payer, 'TestCollection');
      expect(ix.data[0], equals(0));
    });

    test('initCollection encodes name length', () {
      final name = 'FrogNFTs';
      final ix = LendingProgram.initCollection(collection, payer, name);
      final lenView = ByteData.view(ix.data.buffer, 1, 4);
      expect(lenView.getUint32(0, Endian.little), equals(name.length));
    });

    test('placeBid has discriminant 1 and lender signs', () {
      final ix = LendingProgram.placeBid(
        collection, lender, txid, 0,
        BigInt.from(100000), BigInt.from(2592000),
        'tb1q_lender_ord', 'tb1q_lender_pay',
      );
      expect(ix.data[0], equals(1));
      expect(ix.accounts[1].isSigner, isTrue);
    });

    test('acceptBid has discriminant 2', () {
      final ix = LendingProgram.acceptBid(
        collection, borrower,
        'abc123i0', txid, 0,
        'tb1q_borrower_ord', 'tb1q_borrower_pay',
      );
      expect(ix.data[0], equals(2));
    });

    test('repayLoan has discriminant 3', () {
      final repayTxid = Uint8List(32)..fillRange(0, 32, 0xdd);
      final ix = LendingProgram.repayLoan(collection, borrower, 'abc123i0', repayTxid, 0);
      expect(ix.data[0], equals(3));
    });

    test('claimDefault has discriminant 4', () {
      final ix = LendingProgram.claimDefault(collection, lender, 'abc123i0');
      expect(ix.data[0], equals(4));
    });
  });

  // ---- State proof (cross-language vector from Rust himsha_runtime::merkle) ----
  group('StateProof', () {
    const root =
        '54eee82002490e070e17b13ed29afff514ac9249c3a76550759097d58c9b0dab';

    // proof for leaf index 2.
    final proof2 = {
      'state_root': root,
      'leaf':
          'f3ae1a5531bd2bae2efb209184cf11f14e963233167f9d181292ba1e7857cfda',
      'index': 2,
      'siblings': [
        'dbb00c8d0561563563c096b54c39852bb84cd21957bec6e5812c6d5b398b6736',
        '5acfe6cfb257faedb60069ffd4b9da2b4251cf6054d19df7dd40483f090e2167',
      ],
    };

    String hex(List<int> b) =>
        b.map((x) => x.toRadixString(16).padLeft(2, '0')).join();

    test('verifyStateProof passes against the correct root', () {
      expect(verifyStateProof(proof2, root), isTrue);
    });

    test('verifyStateProof fails against a wrong root', () {
      const wrong =
          '00eee82002490e070e17b13ed29afff514ac9249c3a76550759097d58c9b0dab';
      expect(verifyStateProof(proof2, wrong), isFalse);
    });

    test('leafHash matches the Rust vector (key[0]=7, bytes=[1,2,3])', () {
      final key = List<int>.filled(32, 0)..[0] = 7;
      final h = leafHash(key, [1, 2, 3]);
      expect(
        hex(h),
        equals(
            '3be7157c455ae9986535cece016a8df2e1f24c5018a4a49cb4d4d4a31ed28f0f'),
      );
    });

    test('verifyAccountInState passes for leaf 2 real account bytes', () {
      // leaf index 2: key[0]=2, key[1]=0xab, account bytes [2,3,4,5].
      final key = List<int>.filled(32, 0)
        ..[0] = 2
        ..[1] = 0xab;
      expect(
        verifyAccountInState(key, [2, 3, 4, 5], proof2, root),
        isTrue,
      );
    });

    test('verifyAccountInState fails for wrong account bytes', () {
      final key = List<int>.filled(32, 0)
        ..[0] = 2
        ..[1] = 0xab;
      expect(
        verifyAccountInState(key, [9, 9, 9, 9], proof2, root),
        isFalse,
      );
    });
  });
}
