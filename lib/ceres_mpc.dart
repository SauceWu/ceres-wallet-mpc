export 'src/client/mpc_client.dart';
export 'src/client/mpc_exceptions.dart';
export 'src/dto/mpc_dtos.dart';
export 'src/transport/mpc_transport.dart';

/// Lightweight package marker for the Ceres MPC SDK.
class CeresMpcPackage {
  const CeresMpcPackage();

  static const packageName = 'ceres_mpc';
  static const description =
      'Ceres MPC SDK — keygen, recovery, signing, and secure share management.';
}
