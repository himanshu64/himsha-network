import 'dart:convert';
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

Uint8List _str(String s) {
  final bytes = Uint8List.fromList(utf8.encode(s));
  return Uint8List.fromList([..._u32Le(bytes.length), ...bytes]);
}

Uint8List _concat(List<Uint8List> a) {
  final t = a.fold(0, (s, x) => s + x.length);
  final o = Uint8List(t);
  int off = 0;
  for (final x in a) { o.setRange(off, off + x.length, x); off += x.length; }
  return o;
}

class LendingProgram {
  static HimshaInstruction initCollection(
    HimshaPublicKey collection, HimshaPublicKey payer, String name,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.lending,
        accounts: [
          AccountMeta.writable(collection, false),
          AccountMeta.writable(payer, true),
        ],
        data: _concat([Uint8List.fromList([0]), _str(name)]),
      );

  static HimshaInstruction placeBid(
    HimshaPublicKey collection, HimshaPublicKey lender,
    Uint8List bidTxid, int bidVout,
    BigInt loanValue, BigInt loanPeriod,
    String lenderOrdinalsAddr, String lenderPaymentsAddr,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.lending,
        accounts: [
          AccountMeta.writable(collection, false),
          AccountMeta.readonly(lender, true),
        ],
        data: _concat([
          Uint8List.fromList([1]),
          bidTxid, _u32Le(bidVout),
          _u64Le(loanValue), _u64Le(loanPeriod),
          _str(lenderOrdinalsAddr), _str(lenderPaymentsAddr),
        ]),
      );

  static HimshaInstruction acceptBid(
    HimshaPublicKey collection, HimshaPublicKey borrower,
    String inscriptionId, Uint8List inscriptionTxid, int inscriptionVout,
    String borrowerOrdinalsAddr, String borrowerPaymentsAddr,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.lending,
        accounts: [
          AccountMeta.writable(collection, false),
          AccountMeta.readonly(borrower, true),
        ],
        data: _concat([
          Uint8List.fromList([2]),
          _str(inscriptionId),
          inscriptionTxid, _u32Le(inscriptionVout),
          _str(borrowerOrdinalsAddr), _str(borrowerPaymentsAddr),
        ]),
      );

  static HimshaInstruction repayLoan(
    HimshaPublicKey collection, HimshaPublicKey borrower,
    String inscriptionId, Uint8List repayTxid, int repayVout,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.lending,
        accounts: [
          AccountMeta.writable(collection, false),
          AccountMeta.readonly(borrower, true),
        ],
        data: _concat([
          Uint8List.fromList([3]),
          _str(inscriptionId), repayTxid, _u32Le(repayVout),
        ]),
      );

  static HimshaInstruction claimDefault(
    HimshaPublicKey collection, HimshaPublicKey lender, String inscriptionId,
  ) =>
      HimshaInstruction(
        programId: ProgramIds.lending,
        accounts: [
          AccountMeta.writable(collection, false),
          AccountMeta.readonly(lender, true),
        ],
        data: _concat([Uint8List.fromList([4]), _str(inscriptionId)]),
      );
}
