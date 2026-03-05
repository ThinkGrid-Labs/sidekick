/// Sidekick Flutter SDK
///
/// Usage:
/// ```dart
/// final client = SidekickFlutterClient(
///   serverUrl: 'https://flags.example.com',
///   sdkKey: 'my-sdk-key',
/// );
/// await client.init();
///
/// final enabled = client.isEnabled('dark_mode', userId, {'country': 'US'});
/// ```
library sidekick_flutter;

import 'dart:async';
import 'dart:convert';

import 'package:ffi/ffi.dart';
import 'package:http/http.dart' as http;

import 'sidekick_bindings.dart';

class SidekickFlutterClient {
  final String serverUrl;
  final String sdkKey;

  late final SidekickBindings _bindings;
  bool _initialized = false;
  StreamSubscription<String>? _sseSub;

  SidekickFlutterClient({
    required this.serverUrl,
    required this.sdkKey,
  }) : _bindings = SidekickBindings.open();

  /// Initialise: open the SSE stream. The server sends the full flag state on
  /// connect, so no separate REST bootstrap call is needed.
  Future<void> init() async {
    if (_initialized) return;
    _connectDeltas();
    _initialized = true;
  }

  void _connectDeltas() {
    _sseSub?.cancel();

    // Dart doesn't have a built-in SSE client; we use a simple chunked-HTTP
    // stream and parse the SSE text format manually.
    final uri = Uri.parse('$serverUrl/stream');
    final headers = {
      'Authorization': 'Bearer $sdkKey',
      'Accept': 'text/event-stream',
      'Cache-Control': 'no-cache',
    };

    final client = http.Client();

    // Run in the background; reconnect on error.
    _startSseLoop(client, uri, headers);
  }

  void _startSseLoop(
      http.Client client, Uri uri, Map<String, String> headers) async {
    while (true) {
      try {
        final request = http.Request('GET', uri);
        request.headers.addAll(headers);
        final response = await client.send(request);

        if (response.statusCode != 200) {
          throw Exception('SSE connect failed: ${response.statusCode}');
        }

        String eventName = '';
        final buffer = StringBuffer();

        await for (final chunk
            in response.stream.transform(utf8.decoder)) {
          for (final line in chunk.split('\n')) {
            if (line.startsWith('event:')) {
              eventName = line.substring(6).trim();
            } else if (line.startsWith('data:')) {
              buffer.write(line.substring(5).trim());
            } else if (line.isEmpty && buffer.isNotEmpty) {
              final data = buffer.toString();
              buffer.clear();
              _handleSseEvent(eventName, data);
              eventName = '';
            }
          }
        }
      } catch (e) {
        // Stream dropped — wait briefly then reconnect.
        await Future<void>.delayed(const Duration(seconds: 2));
      }
    }
  }

  void _handleSseEvent(String eventName, String data) {
    if (eventName == 'connected') {
      // Clear the Rust cache before the server replays the full state.
      _bindings.sidekick_clear_store();
      return;
    }

    if (eventName == 'update') {
      try {
        final event = jsonDecode(data) as Map<String, dynamic>;

        if (event['type'] == 'UPSERT') {
          final flag = event['flag'] as Map<String, dynamic>;
          _upsertFlag(flag);
        } else if (event['type'] == 'DELETE') {
          _deleteFlag(event['key'] as String);
        }
      } catch (_) {
        // Ignore malformed messages.
      }
    }
  }

  void _upsertFlag(Map<String, dynamic> flag) {
    final key = (flag['key'] as String).toNativeUtf8();
    final rulesJson = jsonEncode(flag['rules'] ?? []).toNativeUtf8();
    final rollout = (flag['rollout_percentage'] as int?) ?? -1;

    _bindings.sidekick_upsert_flag(
      key,
      flag['is_enabled'] as bool,
      rollout,
      rulesJson,
    );

    malloc.free(key);
    malloc.free(rulesJson);
  }

  void _deleteFlag(String key) {
    final k = key.toNativeUtf8();
    _bindings.sidekick_delete_flag(k);
    malloc.free(k);
  }

  /// Evaluate a flag for a user synchronously (sub-microsecond, no network).
  ///
  /// [attributes] is a flat map of string→string user attributes used for
  /// targeting rules (e.g. `{'email': 'u@acme.com', 'country': 'US'}`).
  bool isEnabled(
    String flagKey,
    String userKey, [
    Map<String, String> attributes = const {},
  ]) {
    if (!_initialized) return false;

    final fKey = flagKey.toNativeUtf8();
    final uKey = userKey.toNativeUtf8();
    final attrsJson = jsonEncode(attributes).toNativeUtf8();

    final result =
        _bindings.sidekick_is_enabled(fKey, uKey, attrsJson);

    malloc.free(fKey);
    malloc.free(uKey);
    malloc.free(attrsJson);

    return result != 0;
  }

  /// Cancel the SSE subscription and free resources.
  void close() {
    _sseSub?.cancel();
    _sseSub = null;
  }
}
