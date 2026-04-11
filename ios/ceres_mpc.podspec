#
# To learn more about a Podspec see http://guides.cocoapods.org/syntax/podspec.html.
# Run `pod lib lint ceres_mpc.podspec` to validate before publishing.
#
Pod::Spec.new do |s|
  s.name             = 'ceres_mpc'
  s.version          = '0.1.0'
  s.summary          = 'Two-party ECDSA MPC SDK for Flutter backed by Rust.'
  s.description      = <<-DESC
ceres_mpc is a Flutter FFI plugin that provides two-party ECDSA MPC keygen,
recovery, signing, backup, and key export using a Rust core bridged through
flutter_rust_bridge.
                       DESC
  s.homepage         = 'https://github.com/SauceWu/ceres-mpc'
  s.license          = { :type => 'MIT', :file => '../LICENSE' }
  s.author           = { 'SauceWu' => 'GitHub Issues: https://github.com/SauceWu/ceres-mpc/issues' }
  s.module_name      = 'ceres_mpc'

  # This will ensure the source files in Classes/ are included in the native
  # builds of apps using this FFI plugin. Podspec does not support relative
  # paths, so Classes contains a forwarder C file that relatively imports
  # `../src/*` so that the C sources can be shared among all target platforms.
  s.source           = { :path => '.' }
  s.source_files = 'Classes/**/*'
  s.dependency 'Flutter'
  s.platform = :ios, '11.0'

  # Flutter.framework does not contain a i386 slice.
  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES', 'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386' }
  s.swift_version = '5.0'

  s.script_phase = {
    :name => 'Build or fetch Rust library',
    # First argument is relative path to the `rust` folder, second is name of rust library
    :script => 'sh "$PODS_TARGET_SRCROOT/../cargokit/build_pod.sh" ../rust ceres_mpc',
    :execution_position => :before_compile,
    :input_files => ['${BUILT_PRODUCTS_DIR}/cargokit_phony'],
    # Let XCode know that the static library referenced in -force_load below is
    # created by this build step.
    :output_files => ["${PODS_CONFIGURATION_BUILD_DIR}/ceres_mpc/libceres_mpc.a"],
  }
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    # Flutter.framework does not contain a i386 slice.
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386',
    'OTHER_LDFLAGS' => '-force_load ${PODS_CONFIGURATION_BUILD_DIR}/ceres_mpc/libceres_mpc.a',
  }
end
