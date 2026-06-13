#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: validate-release-bundle.sh <vX.Y.Z> <bundle-dir>" >&2
  exit 2
fi

tag="$1"
bundle_dir="$2"
version="${tag#v}"

if [[ "${tag}" == "${version}" || ! "${version}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "invalid release tag: ${tag}" >&2
  exit 1
fi
if [[ ! -d "${bundle_dir}" ]]; then
  echo "bundle directory not found: ${bundle_dir}" >&2
  exit 1
fi

expected=(
  "FlistWalker-${version}-linux-x86_64"
  "FlistWalker-${version}-linux-x86_64.tar.gz"
  "FlistWalker-${version}-linux-x86_64.README.txt"
  "FlistWalker-${version}-linux-x86_64.LICENSE.txt"
  "FlistWalker-${version}-linux-x86_64.THIRD_PARTY_NOTICES.txt"
  "FlistWalker-${version}-windows-x86_64.exe"
  "FlistWalker-${version}-windows-x86_64.zip"
  "FlistWalker-${version}-windows-x86_64.README.txt"
  "FlistWalker-${version}-windows-x86_64.LICENSE.txt"
  "FlistWalker-${version}-windows-x86_64.THIRD_PARTY_NOTICES.txt"
)
for arch in arm64 x86_64; do
  expected+=(
    "FlistWalker-${version}-macos-${arch}"
    "FlistWalker-${version}-macos-${arch}-app.zip"
    "FlistWalker-${version}-macos-${arch}.tar.gz"
    "FlistWalker-${version}-macos-${arch}.README.txt"
    "FlistWalker-${version}-macos-${arch}.LICENSE.txt"
    "FlistWalker-${version}-macos-${arch}.THIRD_PARTY_NOTICES.txt"
  )
done
expected+=("SHA256SUMS" "SHA256SUMS.sig")

mapfile -t actual < <(find "${bundle_dir}" -mindepth 1 -maxdepth 1 -type f -printf '%f\n' | sort)
mapfile -t expected_sorted < <(printf '%s\n' "${expected[@]}" | sort)
if ! diff -u <(printf '%s\n' "${expected_sorted[@]}") <(printf '%s\n' "${actual[@]}"); then
  echo "release bundle does not contain the exact expected 24 assets" >&2
  exit 1
fi

mapfile -t checksum_files < <(awk '{print $2}' "${bundle_dir}/SHA256SUMS" | sed 's/^\*//' | sort)
mapfile -t distribution_files < <(
  printf '%s\n' "${expected_sorted[@]}" | grep -v -E '^SHA256SUMS(\.sig)?$'
)
if [[ ${#checksum_files[@]} -ne 22 ]] ||
   ! diff -u <(printf '%s\n' "${distribution_files[@]}") <(printf '%s\n' "${checksum_files[@]}"); then
  echo "SHA256SUMS must contain exactly the 22 distribution assets" >&2
  exit 1
fi
(cd "${bundle_dir}" && sha256sum -c SHA256SUMS)

require_zip_entry() {
  local archive="$1"
  local entry="$2"
  unzip -Z1 "${archive}" | grep -Fx -- "${entry}" >/dev/null
}

require_tar_entry() {
  local archive="$1"
  local entry="$2"
  tar -tzf "${archive}" | sed 's#^\./##' | grep -Fx -- "${entry}" >/dev/null
}

windows_zip="${bundle_dir}/FlistWalker-${version}-windows-x86_64.zip"
for entry in flistwalker.exe README.txt LICENSE.txt THIRD_PARTY_NOTICES.txt; do
  require_zip_entry "${windows_zip}" "${entry}"
done

linux_tar="${bundle_dir}/FlistWalker-${version}-linux-x86_64.tar.gz"
for entry in flistwalker README.txt LICENSE.txt THIRD_PARTY_NOTICES.txt; do
  require_tar_entry "${linux_tar}" "${entry}"
done

for arch in arm64 x86_64; do
  mac_tar="${bundle_dir}/FlistWalker-${version}-macos-${arch}.tar.gz"
  for entry in flistwalker README.txt LICENSE.txt THIRD_PARTY_NOTICES.txt; do
    require_tar_entry "${mac_tar}" "${entry}"
  done

  app_name="FlistWalker-${version}-macos-${arch}.app"
  app_zip="${bundle_dir}/FlistWalker-${version}-macos-${arch}-app.zip"
  require_zip_entry "${app_zip}" "${app_name}/Contents/Resources/LICENSE.txt"
  require_zip_entry "${app_zip}" "${app_name}/Contents/Resources/THIRD_PARTY_NOTICES.txt"
done

if find "${bundle_dir}" -mindepth 1 -maxdepth 1 -name '*.app' -print -quit | grep -q .; then
  echo "raw .app bundles must not be uploaded" >&2
  exit 1
fi

echo "release bundle validation passed: ${tag} (${#actual[@]} assets)"
