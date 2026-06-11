import 'dart:typed_data';
import 'package:characters/characters.dart';
import 'package:crypto/crypto.dart';

const String _alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

String _base58Encode(Uint8List bytes) {
  BigInt num = BigInt.zero;
  for (final b in bytes) {
    num = num * BigInt.from(256) + BigInt.from(b);
  }
  var result = '';
  final base = BigInt.from(58);
  while (num > BigInt.zero) {
    final rem = num.remainder(base).toInt();
    result = _alphabet[rem] + result;
    num = num ~/ base;
  }
  for (final b in bytes) {
    if (b == 0) {
      result = '1$result';
    } else {
      break;
    }
  }
  return result;
}

Uint8List _base58Decode(String s) {
  BigInt num = BigInt.zero;
  final base = BigInt.from(58);
  for (final c in s.characters) {
    final idx = _alphabet.indexOf(c);
    if (idx < 0) throw ArgumentError('Invalid base58 character: $c');
    num = num * base + BigInt.from(idx);
  }
  final bytes = <int>[];
  while (num > BigInt.zero) {
    bytes.insert(0, (num & BigInt.from(0xff)).toInt());
    num >>= 8;
  }
  for (final c in s.characters) {
    if (c == '1') {
      bytes.insert(0, 0);
    } else {
      break;
    }
  }
  return Uint8List.fromList(bytes);
}

/// 32-byte identifier for accounts and programs on HIMSHA Network.
class HimshaPublicKey {
  final Uint8List bytes;

  HimshaPublicKey(this.bytes) {
    if (bytes.length != 32) {
      throw ArgumentError('PublicKey must be 32 bytes, got ${bytes.length}');
    }
  }

  /// Create from base58 string.
  factory HimshaPublicKey.fromBase58(String s) => HimshaPublicKey(_base58Decode(s));

  /// Deterministic key from a UTF-8 seed string.
  factory HimshaPublicKey.fromSeed(String seed) {
    final hash = sha256.convert(seed.codeUnits).bytes;
    return HimshaPublicKey(Uint8List.fromList(hash));
  }

  /// Derive a Program Derived Address.
  static (HimshaPublicKey, int) findProgramAddress(
    List<Uint8List> seeds,
    HimshaPublicKey programId,
  ) {
    for (int nonce = 255; nonce >= 0; nonce--) {
      final digest = sha256.convert([
        for (final s in seeds) ...s,
        nonce,
        ...programId.bytes,
        ...'himsha::pda'.codeUnits,
      ]);
      return (HimshaPublicKey(Uint8List.fromList(digest.bytes)), nonce);
    }
    throw StateError('Could not find program address');
  }

  String toBase58() => _base58Encode(bytes);

  @override
  String toString() => toBase58();

  @override
  bool operator ==(Object other) =>
      other is HimshaPublicKey &&
      bytes.length == other.bytes.length &&
      List.generate(32, (i) => bytes[i] == other.bytes[i]).every((b) => b);

  @override
  int get hashCode => Object.hashAll(bytes);
}

/// Well-known program IDs (deterministic).
class ProgramIds {
  static final system      = HimshaPublicKey.fromSeed('himsha::system_program');
  static final token       = HimshaPublicKey.fromSeed('himsha::token_program');
  static final ata         = HimshaPublicKey.fromSeed('himsha::ata_program');
  static final swap        = HimshaPublicKey.fromSeed('himsha::swap_program');
  static final lending     = HimshaPublicKey.fromSeed('himsha::lending_program');
  static final nftMetadata = HimshaPublicKey.fromSeed('himsha::nft_metadata_program');
}
