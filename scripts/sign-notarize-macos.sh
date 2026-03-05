#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/sign-notarize-macos.sh <version> [target-arch] [notary-profile]

Examples:
  FLISTWALKER_MACOS_SIGN_IDENTITY="Developer ID Application: Example Corp (TEAMID1234)" \
    scripts/sign-notarize-macos.sh v0.8.0 arm64 flistwalker-notary

Notes:
  - Run scripts/prepare-release-macos.sh first.
  - Requires a Developer ID Application certificate in keychain.
  - Requires notarytool profile (xcrun notarytool store-credentials).
USAGE
}

if [[ ${1:-} == "-h" || ${1:-} == "--help" || $# -lt 1 || $# -gt 3 ]]; then
  usage
  exit $([[ $# -ge 1 && $# -le 3 ]] && echo 0 || echo 1)
fi

VERSION="$1"
TARGET_ARCH_RAW="${2:-$(uname -m)}"
NOTARY_PROFILE="${3:-${FLISTWALKER_NOTARY_PROFILE:-}}"
SIGN_IDENTITY="${FLISTWALKER_MACOS_SIGN_IDENTITY:-}"

if [[ -z "${SIGN_IDENTITY}" ]]; then
  echo "FLISTWALKER_MACOS_SIGN_IDENTITY を指定してください。" >&2
  echo '例: export FLISTWALKER_MACOS_SIGN_IDENTITY="Developer ID Application: Example Corp (TEAMID1234)"' >&2
  exit 1
fi
if [[ -z "${NOTARY_PROFILE}" ]]; then
  echo "notarytool のプロフィール名を指定してください（第3引数 or FLISTWALKER_NOTARY_PROFILE）。" >&2
  echo "作成例: xcrun notarytool store-credentials flistwalker-notary --apple-id <APPLE_ID> --team-id <TEAM_ID> --password <APP_SPECIFIC_PASSWORD>" >&2
  exit 1
fi
if ! command -v codesign >/dev/null 2>&1; then
  echo "codesign が見つかりません。" >&2
  exit 1
fi
if ! command -v xcrun >/dev/null 2>&1; then
  echo "xcrun が見つかりません。" >&2
  exit 1
fi
if ! xcrun --find notarytool >/dev/null 2>&1; then
  echo "notarytool が見つかりません。Xcode 13+ が必要です。" >&2
  exit 1
fi
if ! xcrun --find stapler >/dev/null 2>&1; then
  echo "stapler が見つかりません。" >&2
  exit 1
fi

case "${TARGET_ARCH_RAW}" in
  aarch64 | arm64)
    ARCH_LABEL="arm64"
    ;;
  x86_64 | amd64)
    ARCH_LABEL="x86_64"
    ;;
  *)
    ARCH_LABEL="${TARGET_ARCH_RAW}"
    ;;
esac

SAFE_VERSION="${VERSION#v}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
OUT_DIR="${REPO_DIR}/dist/${VERSION}"
ASSET_BASENAME="FlistWalker-${SAFE_VERSION}-macos-${ARCH_LABEL}"
APP_NAME="${ASSET_BASENAME}.app"
APP_PATH="${OUT_DIR}/${APP_NAME}"
APP_ZIP_NAME="${ASSET_BASENAME}-app.zip"
APP_ZIP_PATH="${OUT_DIR}/${APP_ZIP_NAME}"
BIN_NAME="${ASSET_BASENAME}"
BIN_PATH="${OUT_DIR}/${BIN_NAME}"
TAR_NAME="${ASSET_BASENAME}.tar.gz"

if [[ ! -d "${APP_PATH}" ]]; then
  echo ".app が見つかりません: ${APP_PATH}" >&2
  echo "先に scripts/prepare-release-macos.sh ${VERSION} を実行してください。" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT
NOTARY_ZIP="${TMP_DIR}/${APP_NAME}.zip"

echo "==> Sign app bundle"
xattr -cr "${APP_PATH}" || true
codesign --force --options runtime --timestamp --sign "${SIGN_IDENTITY}" "${APP_PATH}"
codesign --verify --deep --strict --verbose=2 "${APP_PATH}"

if [[ -f "${BIN_PATH}" ]]; then
  echo "==> Sign standalone binary"
  xattr -cr "${BIN_PATH}" || true
  codesign --force --options runtime --timestamp --sign "${SIGN_IDENTITY}" "${BIN_PATH}"
  codesign --verify --strict --verbose=2 "${BIN_PATH}"
fi

echo "==> Create notarization archive"
ditto -c -k --sequesterRsrc --keepParent "${APP_PATH}" "${NOTARY_ZIP}"

echo "==> Submit to Apple notarization service"
xcrun notarytool submit "${NOTARY_ZIP}" --keychain-profile "${NOTARY_PROFILE}" --wait

echo "==> Staple ticket"
xcrun stapler staple "${APP_PATH}"
xcrun stapler validate "${APP_PATH}"

echo "==> Refresh app zip artifact"
ditto -c -k --sequesterRsrc --keepParent "${APP_PATH}" "${APP_ZIP_PATH}"

echo "==> Refresh SHA256SUMS"
(
  cd "${OUT_DIR}"
  files=()
  [[ -f "${BIN_NAME}" ]] && files+=("${BIN_NAME}")
  [[ -f "${APP_ZIP_NAME}" ]] && files+=("${APP_ZIP_NAME}")
  [[ -f "${TAR_NAME}" ]] && files+=("${TAR_NAME}")
  if [[ ${#files[@]} -eq 0 ]]; then
    echo "hash 対象ファイルがありません。" >&2
    exit 1
  fi
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${files[@]}" > SHA256SUMS
  else
    sha256sum "${files[@]}" > SHA256SUMS
  fi
)

echo "Signed and notarized:"
echo "- ${APP_PATH}"
echo "- ${APP_ZIP_PATH}"
echo "- ${OUT_DIR}/SHA256SUMS"
