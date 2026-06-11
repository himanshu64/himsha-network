import 'dart:typed_data';
import 'package:convert/convert.dart';
import 'package:crypto/crypto.dart';
import 'pubkey.dart';

class AccountMeta {
  final HimshaPublicKey pubkey;
  final bool isSigner;
  final bool isWritable;

  const AccountMeta({
    required this.pubkey,
    required this.isSigner,
    required this.isWritable,
  });

  static AccountMeta writable(HimshaPublicKey pubkey, bool isSigner) =>
      AccountMeta(pubkey: pubkey, isSigner: isSigner, isWritable: true);

  static AccountMeta readonly(HimshaPublicKey pubkey, bool isSigner) =>
      AccountMeta(pubkey: pubkey, isSigner: isSigner, isWritable: false);

  Map<String, dynamic> toJson() => {
        'pubkey':     pubkey.toBase58(),
        'isSigner':   isSigner,
        'isWritable': isWritable,
      };
}

class HimshaInstruction {
  final HimshaPublicKey programId;
  final List<AccountMeta> accounts;
  final Uint8List data;

  const HimshaInstruction({
    required this.programId,
    required this.accounts,
    required this.data,
  });

  Map<String, dynamic> toJson() => {
        'programId': programId.toBase58(),
        'accounts':  accounts.map((a) => a.toJson()).toList(),
        'data':      _hexEncode(data),
      };
}

String _hexEncode(Uint8List bytes) =>
    bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();

class HimshaMessage {
  final List<HimshaPublicKey> signers;
  final List<HimshaInstruction> instructions;
  final BigInt timestamp;

  HimshaMessage({
    required this.signers,
    required this.instructions,
    BigInt? timestamp,
  }) : timestamp = timestamp ??
            BigInt.from(DateTime.now().millisecondsSinceEpoch ~/ 1000);

  Uint8List hash() {
    final sink = AccumulatorSink<Digest>();
    final s = sha256.startChunkedConversion(sink);

    for (final signer in signers) {
      s.add(signer.bytes);
    }
    for (final instr in instructions) {
      s.add(instr.programId.bytes);
      for (final acc in instr.accounts) {
        s.add(acc.pubkey.bytes);
        s.add([acc.isSigner ? 1 : 0, acc.isWritable ? 1 : 0]);
      }
      s.add(instr.data);
    }
    // 8-byte little-endian timestamp
    final tsBytes = ByteData(8)
      ..setUint64(0, timestamp.toInt(), Endian.little);
    s.add(tsBytes.buffer.asUint8List());
    s.close();

    return Uint8List.fromList(sink.events.first.bytes);
  }

  Map<String, dynamic> toJson() => {
        'signers':      signers.map((s) => s.toBase58()).toList(),
        'instructions': instructions.map((i) => i.toJson()).toList(),
        'timestamp':    timestamp.toString(),
      };
}

class HimshaTransaction {
  final int version = 0;
  final HimshaMessage message;
  final List<Uint8List> signatures = [];

  HimshaTransaction(this.message);

  factory HimshaTransaction.create(
    List<HimshaPublicKey> signers,
    List<HimshaInstruction> instructions, {
    BigInt? timestamp,
  }) =>
      HimshaTransaction(HimshaMessage(
        signers: signers,
        instructions: instructions,
        timestamp: timestamp,
      ));

  HimshaTransaction addSignature(Uint8List sig) {
    if (sig.length != 64) {
      throw ArgumentError('Signature must be 64 bytes, got ${sig.length}');
    }
    signatures.add(sig);
    return this;
  }

  Uint8List messageHash() => message.hash();

  Map<String, dynamic> toJson() => {
        'version':    version,
        'signatures': signatures.map(_hexEncode).toList(),
        'message':    message.toJson(),
      };
}
