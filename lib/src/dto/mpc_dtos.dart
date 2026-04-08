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
  bool get isCompleted => status == 'completed';
  bool get isError => status == 'error';
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
}

/// Sign completion result per architecture doc §0.7.
class SignResult {
  final String? signature;
  final String? signedTx;
  final String? txHash;

  const SignResult({
    this.signature,
    this.signedTx,
    this.txHash,
  });

  factory SignResult.fromJson(Map<String, dynamic> json) {
    return SignResult(
      signature: json['signature'] as String?,
      signedTx: json['signedTx'] as String?,
      txHash: json['txHash'] as String?,
    );
  }
}
