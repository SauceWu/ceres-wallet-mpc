import 'package:flutter_test/flutter_test.dart';

import 'package:ceres_mpc/ceres_mpc.dart';

void main() {
  test('exposes package marker metadata', () {
    const package = CeresMpcPackage();
    expect(package, isNotNull);
    expect(CeresMpcPackage.packageName, 'ceres_mpc');
    expect(
      CeresMpcPackage.description,
      contains('MPC SDK'),
    );
  });
}
