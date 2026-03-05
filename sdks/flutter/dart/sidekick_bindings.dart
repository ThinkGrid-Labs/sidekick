// ignore_for_file: non_constant_identifier_names, camel_case_types
/// Raw Dart FFI bindings to libsidekick_flutter.
///
/// These typedefs mirror the C signatures in sdks/flutter/src/lib.rs exactly.
/// Do not call these directly — use [SidekickFlutterClient] instead.
library sidekick_bindings;

import 'dart:ffi';
import 'package:ffi/ffi.dart';

// ---------------------------------------------------------------------------
// Native (C ABI) function signatures
// ---------------------------------------------------------------------------

typedef _UpsertFlagNative = Void Function(
  Pointer<Utf8> key,
  Bool isEnabled,
  Int32 rolloutPercentage,
  Pointer<Utf8> rulesJson,
);

typedef _DeleteFlagNative = Void Function(Pointer<Utf8> key);

typedef _ClearStoreNative = Void Function();

typedef _IsEnabledNative = Int32 Function(
  Pointer<Utf8> flagKey,
  Pointer<Utf8> userKey,
  Pointer<Utf8> attributesJson,
);

// ---------------------------------------------------------------------------
// Dart function types (used by lookupFunction)
// ---------------------------------------------------------------------------

typedef UpsertFlagFn = void Function(
  Pointer<Utf8> key,
  bool isEnabled,
  int rolloutPercentage,
  Pointer<Utf8> rulesJson,
);

typedef DeleteFlagFn = void Function(Pointer<Utf8> key);

typedef ClearStoreFn = void Function();

typedef IsEnabledFn = int Function(
  Pointer<Utf8> flagKey,
  Pointer<Utf8> userKey,
  Pointer<Utf8> attributesJson,
);

// ---------------------------------------------------------------------------
// Binding class — loads symbols from the compiled Rust library.
// ---------------------------------------------------------------------------

class SidekickBindings {
  final UpsertFlagFn sidekick_upsert_flag;
  final DeleteFlagFn sidekick_delete_flag;
  final ClearStoreFn sidekick_clear_store;
  final IsEnabledFn sidekick_is_enabled;

  SidekickBindings(DynamicLibrary lib)
      : sidekick_upsert_flag = lib.lookupFunction<_UpsertFlagNative, UpsertFlagFn>(
            'sidekick_upsert_flag'),
        sidekick_delete_flag = lib.lookupFunction<_DeleteFlagNative, DeleteFlagFn>(
            'sidekick_delete_flag'),
        sidekick_clear_store = lib.lookupFunction<_ClearStoreNative, ClearStoreFn>(
            'sidekick_clear_store'),
        sidekick_is_enabled = lib.lookupFunction<_IsEnabledNative, IsEnabledFn>(
            'sidekick_is_enabled');

  /// Opens the correct shared library for the current platform.
  factory SidekickBindings.open() {
    final DynamicLibrary lib;

    // ignore: do_not_use_environment
    if (const bool.fromEnvironment('dart.vm.product')) {
      // iOS static link — symbols are already in the process image.
      lib = DynamicLibrary.process();
    } else {
      // Android / desktop dynamic library.
      lib = DynamicLibrary.open('libsidekick_flutter.so');
    }

    return SidekickBindings(lib);
  }
}
