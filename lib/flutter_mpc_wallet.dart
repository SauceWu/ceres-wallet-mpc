export 'src/client/mpc_client.dart';
export 'src/client/mpc_exceptions.dart';
export 'src/dto/mpc_dtos.dart';
export 'src/transport/mpc_transport.dart';

/// Lightweight package marker for the standalone Flutter MPC wallet project.
class FlutterMpcWalletPackage {
  const FlutterMpcWalletPackage();

  static const packageName = 'flutter_mpc_wallet';
  static const description =
      'Standalone Flutter package for MPC wallet orchestration and share management.';
}
