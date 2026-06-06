import 'dart:typed_data';
import '../pubkey.dart';
import '../transaction.dart';

Uint8List _u64Le(BigInt n) {
  final buf = ByteData(8)..setUint64(0, n.toInt(), Endian.little);
  return buf.buffer.asUint8List();
}

Uint8List _concat(List<Uint8List> a) {
  final t = a.fold(0, (s, x) => s + x.length);
  final o = Uint8List(t);
  int off = 0;
  for (final x in a) { o.setRange(off, off + x.length, x); off += x.length; }
  return o;
}

class SwapProgram {
  static HimshaInstruction initialize(
    HimshaPublicKey pool, HimshaPublicKey mintA, HimshaPublicKey mintB,
    HimshaPublicKey resA, HimshaPublicKey resB, HimshaPublicKey lpMint,
    HimshaPublicKey payer, BigInt feeNum, BigInt feeDen,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.swap,
        accounts: [
          AccountMeta.writable(pool, false),
          AccountMeta.readonly(mintA, false),
          AccountMeta.readonly(mintB, false),
          AccountMeta.writable(resA, false),
          AccountMeta.writable(resB, false),
          AccountMeta.writable(lpMint, false),
          AccountMeta.writable(payer, true),
        ],
        data: _concat([Uint8List.fromList([0]), _u64Le(feeNum), _u64Le(feeDen)]),
      );

  static HimshaInstruction swap(
    HimshaPublicKey pool, HimshaPublicKey source, HimshaPublicKey dest,
    HimshaPublicKey resIn, HimshaPublicKey resOut, HimshaPublicKey user,
    BigInt amountIn, BigInt minOut,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.swap,
        accounts: [
          AccountMeta.readonly(pool, false),
          AccountMeta.writable(source, false),
          AccountMeta.writable(dest, false),
          AccountMeta.writable(resIn, false),
          AccountMeta.writable(resOut, false),
          AccountMeta.readonly(user, true),
        ],
        data: _concat([Uint8List.fromList([1]), _u64Le(amountIn), _u64Le(minOut)]),
      );

  static HimshaInstruction deposit(
    HimshaPublicKey pool, HimshaPublicKey userA, HimshaPublicKey userB,
    HimshaPublicKey resA, HimshaPublicKey resB, HimshaPublicKey userLp,
    HimshaPublicKey user, BigInt maxA, BigInt maxB, BigInt minLp,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.swap,
        accounts: [
          AccountMeta.writable(pool, false),
          AccountMeta.writable(userA, false), AccountMeta.writable(userB, false),
          AccountMeta.writable(resA, false),  AccountMeta.writable(resB, false),
          AccountMeta.writable(userLp, false), AccountMeta.readonly(user, true),
        ],
        data: _concat([Uint8List.fromList([2]), _u64Le(maxA), _u64Le(maxB), _u64Le(minLp)]),
      );

  static HimshaInstruction withdraw(
    HimshaPublicKey pool, HimshaPublicKey userA, HimshaPublicKey userB,
    HimshaPublicKey resA, HimshaPublicKey resB, HimshaPublicKey userLp,
    HimshaPublicKey user, BigInt lp, BigInt minA, BigInt minB,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.swap,
        accounts: [
          AccountMeta.writable(pool, false),
          AccountMeta.writable(userA, false), AccountMeta.writable(userB, false),
          AccountMeta.writable(resA, false),  AccountMeta.writable(resB, false),
          AccountMeta.writable(userLp, false), AccountMeta.readonly(user, true),
        ],
        data: _concat([Uint8List.fromList([3]), _u64Le(lp), _u64Le(minA), _u64Le(minB)]),
      );
}
