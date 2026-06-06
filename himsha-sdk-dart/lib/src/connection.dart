import 'dart:convert';
import 'package:http/http.dart' as http;
import 'pubkey.dart';
import 'transaction.dart';

class HimshaRpcException implements Exception {
  final int code;
  final String message;
  HimshaRpcException(this.code, this.message);
  @override
  String toString() => 'HimshaRpcException($code): $message';
}

class AccountInfo {
  final String key;
  final BigInt lamports;
  final String dataHex;
  final String owner;
  final bool executable;

  const AccountInfo({
    required this.key,
    required this.lamports,
    required this.dataHex,
    required this.owner,
    required this.executable,
  });

  factory AccountInfo.fromJson(Map<String, dynamic> j) => AccountInfo(
        key:        j['key'] as String,
        lamports:   BigInt.parse(j['lamports'].toString()),
        dataHex:    j['data'] as String,
        owner:      j['owner'] as String,
        executable: j['executable'] as bool,
      );
}

class UtxoInfo {
  final String txid;
  final int vout;
  final BigInt value;
  final String scriptPubkey;
  final int confirmations;

  const UtxoInfo({
    required this.txid,
    required this.vout,
    required this.value,
    required this.scriptPubkey,
    required this.confirmations,
  });

  factory UtxoInfo.fromJson(Map<String, dynamic> j) {
    final meta = j['meta'] as Map<String, dynamic>;
    return UtxoInfo(
      txid:          meta['txid'] as String,
      vout:          meta['vout'] as int,
      value:         BigInt.parse(j['value'].toString()),
      scriptPubkey:  j['scriptPubkey'] as String,
      confirmations: j['confirmations'] as int,
    );
  }
}

/// HIMSHA Network JSON-RPC client for Dart.
///
/// ```dart
/// final conn = HimshaConnection('http://localhost:9100');
/// final ready = await conn.isNodeReady();
/// ```
class HimshaConnection {
  final String endpoint;
  int _nextId = 1;
  final http.Client _client;

  HimshaConnection(this.endpoint, {http.Client? client})
      : _client = client ?? http.Client();

  Future<T> _call<T>(String method, [List<dynamic> params = const []]) async {
    final id = _nextId++;
    final body = jsonEncode({
      'jsonrpc': '2.0',
      'id': id,
      'method': method,
      'params': params,
    });

    final response = await _client.post(
      Uri.parse(endpoint),
      headers: {'Content-Type': 'application/json'},
      body: body,
    );

    if (response.statusCode != 200) {
      throw HimshaRpcException(response.statusCode, 'HTTP ${response.statusCode}');
    }

    final json = jsonDecode(response.body) as Map<String, dynamic>;
    if (json.containsKey('error')) {
      final err = json['error'] as Map<String, dynamic>;
      throw HimshaRpcException(err['code'] as int, err['message'] as String);
    }
    return json['result'] as T;
  }

  // ---- Node queries ----

  Future<bool> isNodeReady() => _call<bool>('himsha_isNodeReady');

  Future<BigInt> getSlot() async {
    final slot = await _call<dynamic>('himsha_getSlot');
    return BigInt.parse(slot.toString());
  }

  Future<Map<String, dynamic>?> getBlock(BigInt slot) =>
      _call<Map<String, dynamic>?>('himsha_getBlock', [slot.toString()]);

  Future<List<String>> listPrograms() =>
      _call<List<dynamic>>('himsha_listPrograms').then((l) => l.cast<String>());

  // ---- Account queries ----

  Future<AccountInfo?> getAccountInfo(HimshaPublicKey pubkey) async {
    final raw = await _call<Map<String, dynamic>?>('himsha_getAccountInfo', [pubkey.toBase58()]);
    return raw == null ? null : AccountInfo.fromJson(raw);
  }

  Future<List<AccountInfo>> getProgramAccounts(HimshaPublicKey programId) async {
    final raw = await _call<List<dynamic>>('himsha_getProgramAccounts', [programId.toBase58()]);
    return raw
        .cast<Map<String, dynamic>>()
        .map(AccountInfo.fromJson)
        .toList();
  }

  Future<bool> accountExists(HimshaPublicKey pubkey) async =>
      (await getAccountInfo(pubkey)) != null;

  // ---- Bitcoin ----

  Future<UtxoInfo?> getUtxo(String txid, int vout) async {
    final raw = await _call<Map<String, dynamic>?>('himsha_getUtxo', [txid, vout]);
    return raw == null ? null : UtxoInfo.fromJson(raw);
  }

  // ---- Transactions ----

  Future<String> sendTransaction(HimshaTransaction tx) =>
      _call<String>('himsha_sendTransaction', [tx.toJson()]);

  Future<String> deployProgram(String elfHex, String imageIdHex) =>
      _call<String>('himsha_deployProgram', [elfHex, imageIdHex]);

  Future<BigInt> confirmTransaction(String txId, {Duration timeout = const Duration(seconds: 30)}) async {
    final deadline = DateTime.now().add(timeout);
    while (DateTime.now().isBefore(deadline)) {
      final slot = await getSlot();
      final block = await getBlock(slot);
      if (block != null) {
        final txs = (block['transactions'] as List?) ?? [];
        for (final t in txs) {
          if ((t as Map)['id'] == txId) return slot;
        }
      }
      await Future.delayed(const Duration(seconds: 1));
    }
    throw TimeoutException('Transaction $txId not confirmed within $timeout', timeout);
  }

  void close() => _client.close();
}

class TimeoutException implements Exception {
  final String message;
  final Duration? timeout;
  TimeoutException(this.message, [this.timeout]);
  @override
  String toString() => 'TimeoutException: $message';
}
