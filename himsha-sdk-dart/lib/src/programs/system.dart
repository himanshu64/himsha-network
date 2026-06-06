import 'dart:typed_data';
import '../pubkey.dart';
import '../transaction.dart';

Uint8List _u64Le(BigInt n) {
  final buf = ByteData(8)..setUint64(0, n.toInt(), Endian.little);
  return buf.buffer.asUint8List();
}

Uint8List _u32Le(int n) {
  final buf = ByteData(4)..setUint32(0, n, Endian.little);
  return buf.buffer.asUint8List();
}

Uint8List _concat(List<Uint8List> arrays) {
  final total = arrays.fold(0, (s, a) => s + a.length);
  final out = Uint8List(total);
  int offset = 0;
  for (final a in arrays) {
    out.setRange(offset, offset + a.length, a);
    offset += a.length;
  }
  return out;
}

class SystemProgram {
  static HimshaInstruction createAccount(
    HimshaPublicKey payer,
    HimshaPublicKey newAccount,
    BigInt lamports,
    BigInt space,
    HimshaPublicKey owner,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.system,
        accounts: [
          AccountMeta.writable(payer, true),
          AccountMeta.writable(newAccount, true),
        ],
        data: _concat([Uint8List.fromList([0]), _u64Le(lamports), _u64Le(space), owner.bytes]),
      );

  static HimshaInstruction transfer(
    HimshaPublicKey from,
    HimshaPublicKey to,
    BigInt lamports,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.system,
        accounts: [
          AccountMeta.writable(from, true),
          AccountMeta.writable(to, false),
        ],
        data: _concat([Uint8List.fromList([2]), _u64Le(lamports)]),
      );

  static HimshaInstruction assign(HimshaPublicKey account, HimshaPublicKey owner) =>
      HimshaInstruction(
        programId: ProgramIds.system,
        accounts: [AccountMeta.writable(account, true)],
        data: _concat([Uint8List.fromList([3]), owner.bytes]),
      );

  static HimshaInstruction allocate(HimshaPublicKey account, BigInt space) =>
      HimshaInstruction(
        programId: ProgramIds.system,
        accounts: [AccountMeta.writable(account, true)],
        data: _concat([Uint8List.fromList([4]), _u64Le(space)]),
      );
}
