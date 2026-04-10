import 'package:flutter_test/flutter_test.dart';
import 'package:ceres_mpc_example/main.dart';

void main() {
  testWidgets('renders example app actions', (WidgetTester tester) async {
    await tester.pumpWidget(const ExampleApp());

    expect(find.text('ceres_mpc Example'), findsOneWidget);
    expect(find.text('Transport Mode'), findsOneWidget);
    expect(find.text('Mock'), findsOneWidget);
    expect(find.text('HTTP'), findsOneWidget);
    expect(find.text('WebSocket'), findsOneWidget);
    expect(find.text('1. Keygen'), findsOneWidget);
    expect(find.text('2. Sign'), findsOneWidget);
    expect(find.text('3. Recovery'), findsOneWidget);
    expect(find.text('4. Export'), findsOneWidget);
    expect(find.text('Error Handling'), findsOneWidget);
  });
}
