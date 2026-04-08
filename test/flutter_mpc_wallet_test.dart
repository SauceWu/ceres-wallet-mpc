import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_mpc_wallet/flutter_mpc_wallet.dart';

void main() {
  test('exposes package marker metadata', () {
    const package = FlutterMpcWalletPackage();
    expect(package, isNotNull);
    expect(FlutterMpcWalletPackage.packageName, 'flutter_mpc_wallet');
    expect(
      FlutterMpcWalletPackage.description,
      contains('MPC wallet orchestration'),
    );
  });
}
