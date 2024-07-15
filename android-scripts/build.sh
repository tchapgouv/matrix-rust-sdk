#!/usr/bin/env bash
set -e

helpFunction() {
  echo ""
  echo "Usage: $0 -r -p /Documents/dev/matrix-rust-sdk -m sdk"
  echo -e "\t-p Local path to the rust-sdk repository"
  echo -e "\t-o Optional output path with the expected name of the aar file"
  echo -e "\t-r Flag to build in release mode"
  echo -e "\t-t Option to to select an android target to build against. Default will build for all targets."
  exit 1
}

scripts_dir=$(
  cd "$(dirname "${BASH_SOURCE[0]}")" || exit
  pwd -P
)

is_release='false'
only_target=''
output=''

while getopts ':rp:t:o:' 'opt'; do
  case ${opt} in
  'r') is_release='true' ;;
  'p') sdk_path="$OPTARG" ;;
  't') only_target="$OPTARG" ;;
  'o') output="$OPTARG" ;;
  ?) helpFunction ;;
  esac
done

if [ -z "$sdk_path" ]; then
  echo "sdk_path is empty, please provide one"
  helpFunction
fi

if [ -z "$only_target" ]; then
  echo "no target provided, build for all targets"
  target_command=()
else
  target_command=("--only-target" "$only_target")
fi

if ${is_release}; then
  profile="release"
else
  profile="reldbg"
fi

src_dir="$scripts_dir/../bindings/android/src/main"
package="full-sdk"

echo "Launching build script with following params:"
echo "sdk_path = $sdk_path"
echo "profile = $profile"
echo "src-dir = $src_dir"

pushd "$sdk_path" || exit 1

cargo xtask kotlin build-android-library --profile "$profile" "${target_command[@]}" --src-dir "$src_dir" --package "$package"

pushd "$scripts_dir/.." || exit 1

shift $((OPTIND - 1))

moveFunction() {
  if [ -z "$output" ]; then
    echo "No output path provided, keep the generated path"
  else
    mv "$1" "$output"
  fi
}

## For now, cargo ndk includes all generated so files from the target directory, so makes sure it just includes the one we need.
echo "Clean .so files"
find bindings/android/src/main/jniLibs -type f ! -name 'libmatrix_sdk_ffi.so' -delete

if ${is_release}; then
  ./gradlew :bindings:android:assembleRelease
  moveFunction "bindings/android/build/outputs/aar/sdk-android-release.aar"
else
  ./gradlew :bindings:android:assembleDebug
  moveFunction "bindings/android/build/outputs/aar/sdk-android-debug.aar"
fi
