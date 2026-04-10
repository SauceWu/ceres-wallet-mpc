/// Dart DTOs mirroring Rust MPC types and architecture doc field contracts.
///
/// JSON keys use snake_case to match Rust serde_json output.
library;

/// Round-level result returned by Rust stub functions via JSON serialization.
class MpcRoundResult {
  final String status;
  final int round;
  final String? clientPayload;
  final String? errorMessage;

  const MpcRoundResult({
    required this.status,
    required this.round,
    this.clientPayload,
    this.errorMessage,
  });

  factory MpcRoundResult.fromJson(Map<String, dynamic> json) {
    return MpcRoundResult(
      status: json['status'] as String,
      round: json['round'] as int,
      clientPayload: json['client_payload'] as String?,
      errorMessage: json['error_message'] as String?,
    );
  }

  Map<String, dynamic> toJson() => {
        'status': status,
        'round': round,
        'client_payload': clientPayload,
        'error_message': errorMessage,
      };

  bool get isContinue => status == 'continue';
  bool get isCompleted => status == 'completed' || status == 'completed_with_message';
  bool get isCompletedWithMessage => status == 'completed_with_message';
  bool get isError => status == 'error';

  @override
  String toString() {
    return 'MpcRoundResult('
        'status: $status, '
        'round: $round, '
        'clientPayload: $clientPayload, '
        'errorMessage: $errorMessage'
        ')';
  }
}

/// Keygen completion result per architecture doc §0.7.
class KeygenResult {
  final String mpcKeyId;
  final String address;
  final String publicKey;
  final String curve;
  final int threshold;
  final String keyRef;
  final String backupState;
  final int rotationVersion;
  final String localEncryptedShare;
  final String? encryptedBackupShare;

  const KeygenResult({
    required this.mpcKeyId,
    required this.address,
    required this.publicKey,
    required this.curve,
    required this.threshold,
    required this.keyRef,
    required this.backupState,
    required this.rotationVersion,
    required this.localEncryptedShare,
    this.encryptedBackupShare,
  });

  factory KeygenResult.fromJson(Map<String, dynamic> json) {
    return KeygenResult(
      mpcKeyId: json['mpcKeyId'] as String,
      address: json['address'] as String,
      publicKey: json['publicKey'] as String,
      curve: json['curve'] as String,
      threshold: json['threshold'] as int,
      keyRef: json['keyRef'] as String,
      backupState: json['backupState'] as String,
      rotationVersion: json['rotationVersion'] as int,
      localEncryptedShare: json['localEncryptedShare'] as String,
      encryptedBackupShare: json['encryptedBackupShare'] as String?,
    );
  }

  @override
  String toString() {
    return 'KeygenResult('
        'mpcKeyId: $mpcKeyId, '
        'address: $address, '
        'publicKey: $publicKey, '
        'curve: $curve, '
        'threshold: $threshold, '
        'keyRef: $keyRef, '
        'backupState: $backupState, '
        'rotationVersion: $rotationVersion, '
        'localEncryptedShare: [REDACTED], '
        'encryptedBackupShare: [REDACTED]'
        ')';
  }
}

/// Recovery completion result per architecture doc §0.7.
class RecoveryResult {
  final String mpcKeyId;
  final String address;
  final String publicKey;
  final int rotationVersion;
  final String localEncryptedShare;
  final String? encryptedBackupShare;

  const RecoveryResult({
    required this.mpcKeyId,
    required this.address,
    required this.publicKey,
    required this.rotationVersion,
    required this.localEncryptedShare,
    this.encryptedBackupShare,
  });

  factory RecoveryResult.fromJson(Map<String, dynamic> json) {
    return RecoveryResult(
      mpcKeyId: json['mpcKeyId'] as String,
      address: json['address'] as String,
      publicKey: json['publicKey'] as String,
      rotationVersion: json['rotationVersion'] as int,
      localEncryptedShare: json['localEncryptedShare'] as String,
      encryptedBackupShare: json['encryptedBackupShare'] as String?,
    );
  }

  @override
  String toString() {
    return 'RecoveryResult('
        'mpcKeyId: $mpcKeyId, '
        'address: $address, '
        'publicKey: $publicKey, '
        'rotationVersion: $rotationVersion, '
        'localEncryptedShare: [REDACTED], '
        'encryptedBackupShare: [REDACTED]'
        ')';
  }
}

/// Sign completion result per D-02.
/// Contains ECDSA signature components: r, s, recid.
/// Caller is responsible for assembling signedTx.
class SignResult {
  final String r;
  final String s;
  final int recid;

  const SignResult({
    required this.r,
    required this.s,
    required this.recid,
  });

  factory SignResult.fromJson(Map<String, dynamic> json) {
    return SignResult(
      r: json['r'] as String,
      s: json['s'] as String,
      recid: json['recid'] as int,
    );
  }

  @override
  String toString() {
    return 'SignResult(r: [REDACTED], s: [REDACTED], recid: $recid)';
  }
}

/// Result of exporting MPC wallet to a standard wallet.
/// Contains the full private key reconstructed from both party shares.
class ExportResult {
  final String privateKey;
  final String address;
  final bool exported;

  const ExportResult({
    required this.privateKey,
    required this.address,
    required this.exported,
  });

  factory ExportResult.fromJson(Map<String, dynamic> json) {
    return ExportResult(
      privateKey: json['privateKey'] ?? json['private_key'] as String,
      address: json['address'] as String,
      exported: json['exported'] as bool,
    );
  }

  @override
  String toString() {
    return 'ExportResult(privateKey: [REDACTED], address: $address, exported: $exported)';
  }
}

/// Backup envelope containing encrypted share material.
/// Generated by Rust-side deriveBackupEnvelope.
/// Phase 2 stub — real AES-256-GCM encryption in Phase 5.
class BackupEnvelope {
  final String version;
  final String algorithm;
  final String createdAt;
  final String payload;

  const BackupEnvelope({
    required this.version,
    required this.algorithm,
    required this.createdAt,
    required this.payload,
  });

  /// Parses JSON with snake_case keys from Rust serde_json output.
  /// Note: 'created_at' (Rust) maps to 'createdAt' (Dart).
  factory BackupEnvelope.fromJson(Map<String, dynamic> json) {
    return BackupEnvelope(
      version: json['version'] as String,
      algorithm: json['algorithm'] as String,
      createdAt: json['created_at'] as String,
      payload: json['payload'] as String,
    );
  }

  @override
  String toString() {
    return 'BackupEnvelope('
        'version: $version, '
        'algorithm: $algorithm, '
        'createdAt: $createdAt, '
        'payload: [REDACTED]'
        ')';
  }
}
