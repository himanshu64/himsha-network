import 'dart:typed_data';
import 'package:crypto/crypto.dart';

/// State-root inclusion proof, as returned by `himsha_getStateProof`.
///
/// Mirrors the Rust `himsha_runtime::merkle` tree exactly so a client can
/// verify, with no trust in the node, that an account is committed in the state
/// root — and, when the root matches `anchored_state_root`, in the
/// Bitcoin-anchored state.
///
/// Proofs are passed around as the raw decoded JSON `Map` (snake_case keys, as
/// served on the wire): `state_root`, `leaf`, `index`, `siblings`,
/// `anchored_slot`, `anchored_state_root`, `anchored_btc_txid`.

const _leafTag = 0x00;
const _nodeTag = 0x01;

List<int> _sha256(List<int> bytes) => sha256.convert(bytes).bytes;

List<int> _fromHex(String hex) {
  if (hex.length % 2 != 0) {
    throw FormatException('odd-length hex: $hex');
  }
  final out = Uint8List(hex.length ~/ 2);
  for (var i = 0; i < out.length; i++) {
    out[i] = int.parse(hex.substring(i * 2, i * 2 + 2), radix: 16);
  }
  return out;
}

String _toHex(List<int> bytes) {
  final sb = StringBuffer();
  for (final b in bytes) {
    sb.write(b.toRadixString(16).padLeft(2, '0'));
  }
  return sb.toString();
}

/// Leaf hash for an account: `SHA-256(0x00 ‖ key ‖ accountBytes)`.
List<int> leafHash(List<int> key, List<int> accountBytes) =>
    _sha256(<int>[_leafTag, ...key, ...accountBytes]);

/// Internal-node hash: `SHA-256(0x01 ‖ left ‖ right)`.
List<int> _nodeHash(List<int> left, List<int> right) =>
    _sha256(<int>[_nodeTag, ...left, ...right]);

/// Recompute the root a proof attests to and compare against [rootHex].
///
/// Matches `MerkleProof::verify` in Rust and `verifyStateProof` in the TS SDK:
/// at each level the sibling sits on the right when the running index is even,
/// on the left when odd.
bool verifyStateProof(Map proof, String rootHex) {
  var h = _fromHex(proof['leaf'] as String);
  var idx = (proof['index'] as num).toInt();
  final siblings = (proof['siblings'] as List).cast<String>();
  for (final sibHex in siblings) {
    final sib = _fromHex(sibHex);
    h = (idx & 1) == 0 ? _nodeHash(h, sib) : _nodeHash(sib, h);
    idx >>= 1;
  }
  return _toHex(h) == rootHex.toLowerCase();
}

/// Verify that [accountBytes] (the borsh-encoded StoredAccount the client holds
/// for [key]) is the exact value committed under [proof].
///
/// This is the strongest check: it ties the proof's leaf to the caller's own
/// account bytes, then walks the tree to the root — so a node cannot serve a
/// proof for a different value.
bool verifyAccountInState(
  List<int> key,
  List<int> accountBytes,
  Map proof,
  String rootHex,
) {
  if (_toHex(leafHash(key, accountBytes)) !=
      (proof['leaf'] as String).toLowerCase()) {
    return false;
  }
  return verifyStateProof(proof, rootHex);
}
