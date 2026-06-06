import 'dart:typed_data';
import '../pubkey.dart';
import '../transaction.dart';

Uint8List _u64Le(BigInt n) {
  final buf = ByteData(8)..setUint64(0, n.toInt(), Endian.little);
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

Uint8List _optKey(HimshaPublicKey? key) =>
    key != null ? _concat([Uint8List.fromList([1]), key.bytes]) : Uint8List.fromList([0]);

class TokenProgram {
  static HimshaInstruction initializeMint(
    HimshaPublicKey mint,
    HimshaPublicKey authority,
    int decimals, [
    HimshaPublicKey? freezeAuthority,
  ]) =>
      HimshaInstruction(
        programId: ProgramIds.token,
        accounts: [AccountMeta.writable(mint, false)],
        data: _concat([
          Uint8List.fromList([0, decimals]),
          authority.bytes,
          _optKey(freezeAuthority),
        ]),
      );

  static HimshaInstruction initializeAccount(
    HimshaPublicKey account,
    HimshaPublicKey mint,
    HimshaPublicKey owner,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.token,
        accounts: [
          AccountMeta.writable(account, false),
          AccountMeta.readonly(mint, false),
          AccountMeta.readonly(owner, false),
        ],
        data: Uint8List.fromList([1]),
      );

  static HimshaInstruction mintTo(
    HimshaPublicKey mint,
    HimshaPublicKey destination,
    HimshaPublicKey authority,
    BigInt amount,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.token,
        accounts: [
          AccountMeta.writable(mint, false),
          AccountMeta.writable(destination, false),
          AccountMeta.readonly(authority, true),
        ],
        data: _concat([Uint8List.fromList([2]), _u64Le(amount)]),
      );

  static HimshaInstruction transfer(
    HimshaPublicKey source,
    HimshaPublicKey destination,
    HimshaPublicKey owner,
    BigInt amount,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.token,
        accounts: [
          AccountMeta.writable(source, false),
          AccountMeta.writable(destination, false),
          AccountMeta.readonly(owner, true),
        ],
        data: _concat([Uint8List.fromList([3]), _u64Le(amount)]),
      );

  static HimshaInstruction burn(
    HimshaPublicKey account,
    HimshaPublicKey mint,
    HimshaPublicKey owner,
    BigInt amount,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.token,
        accounts: [
          AccountMeta.writable(account, false),
          AccountMeta.writable(mint, false),
          AccountMeta.readonly(owner, true),
        ],
        data: _concat([Uint8List.fromList([4]), _u64Le(amount)]),
      );

  static HimshaInstruction closeAccount(
    HimshaPublicKey account,
    HimshaPublicKey destination,
    HimshaPublicKey owner,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.token,
        accounts: [
          AccountMeta.writable(account, false),
          AccountMeta.writable(destination, false),
          AccountMeta.readonly(owner, true),
        ],
        data: Uint8List.fromList([9]),
      );
}
