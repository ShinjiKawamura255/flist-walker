#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work_dir="$(mktemp -d)"
trap 'rm -rf "${work_dir}"' EXIT

tag="v9.8.7"
version="${tag#v}"
bundle="${work_dir}/bundle"
mkdir -p "${bundle}"

make_sidecars() {
  local base="$1"
  printf 'readme\n' >"${bundle}/${base}.README.txt"
  printf 'license\n' >"${bundle}/${base}.LICENSE.txt"
  printf 'notices\n' >"${bundle}/${base}.THIRD_PARTY_NOTICES.txt"
}

windows_base="FlistWalker-${version}-windows-x86_64"
printf 'windows binary\n' >"${bundle}/${windows_base}.exe"
printf 'windows cli binary\n' >"${bundle}/fw-${version}-windows-x86_64.exe"
make_sidecars "${windows_base}"
windows_archive="${work_dir}/windows"
mkdir -p "${windows_archive}"
printf 'windows binary\n' >"${windows_archive}/flistwalker.exe"
printf 'readme\n' >"${windows_archive}/README.txt"
printf 'license\n' >"${windows_archive}/LICENSE.txt"
printf 'notices\n' >"${windows_archive}/THIRD_PARTY_NOTICES.txt"
(cd "${windows_archive}" && zip -q "${bundle}/${windows_base}.zip" ./*)

linux_base="FlistWalker-${version}-linux-x86_64"
printf 'linux binary\n' >"${bundle}/${linux_base}"
printf 'linux cli binary\n' >"${bundle}/fw-${version}-linux-x86_64"
make_sidecars "${linux_base}"
linux_archive="${work_dir}/linux"
mkdir -p "${linux_archive}"
printf 'linux binary\n' >"${linux_archive}/flistwalker"
printf 'readme\n' >"${linux_archive}/README.txt"
printf 'license\n' >"${linux_archive}/LICENSE.txt"
printf 'notices\n' >"${linux_archive}/THIRD_PARTY_NOTICES.txt"
(cd "${linux_archive}" && tar -czf "${bundle}/${linux_base}.tar.gz" ./*)

for arch in arm64 x86_64; do
  mac_base="FlistWalker-${version}-macos-${arch}"
  printf 'mac binary\n' >"${bundle}/${mac_base}"
  printf 'mac cli binary\n' >"${bundle}/fw-${version}-macos-${arch}"
  make_sidecars "${mac_base}"

  mac_archive="${work_dir}/mac-${arch}"
  mkdir -p "${mac_archive}"
  printf 'mac binary\n' >"${mac_archive}/flistwalker"
  printf 'readme\n' >"${mac_archive}/README.txt"
  printf 'license\n' >"${mac_archive}/LICENSE.txt"
  printf 'notices\n' >"${mac_archive}/THIRD_PARTY_NOTICES.txt"
  (cd "${mac_archive}" && tar -czf "${bundle}/${mac_base}.tar.gz" ./*)

  app_name="${mac_base}.app"
  app_root="${work_dir}/${app_name}"
  mkdir -p "${app_root}/Contents/Resources"
  printf 'license\n' >"${app_root}/Contents/Resources/LICENSE.txt"
  printf 'notices\n' >"${app_root}/Contents/Resources/THIRD_PARTY_NOTICES.txt"
  (cd "${work_dir}" && zip -qr "${bundle}/${mac_base}-app.zip" "${app_name}")
done

(cd "${bundle}" && sha256sum FlistWalker-* fw-* >SHA256SUMS)
printf 'test signature\n' >"${bundle}/SHA256SUMS.sig"

bash "${repo_dir}/scripts/validate-release-bundle.sh" "${tag}" "${bundle}"

refresh_checksums() {
  (cd "${bundle}" && sha256sum FlistWalker-* fw-* >SHA256SUMS)
}

assert_archive_rejects_extra_member() {
  local archive="$1"
  local label="$2"
  local backup="$3"
  if bash "${repo_dir}/scripts/validate-release-bundle.sh" "${tag}" "${bundle}" >/dev/null 2>&1; then
    echo "validator accepted an extra fw member in ${label}" >&2
    exit 1
  fi
  cp "${backup}" "${archive}"
  refresh_checksums
}

windows_backup="${work_dir}/windows-zip.backup"
cp "${bundle}/${windows_base}.zip" "${windows_backup}"
printf 'unexpected cli binary\n' >"${windows_archive}/fw.exe"
(cd "${windows_archive}" && zip -q -u "${bundle}/${windows_base}.zip" fw.exe)
refresh_checksums
assert_archive_rejects_extra_member "${bundle}/${windows_base}.zip" windows-zip "${windows_backup}"

linux_backup="${work_dir}/linux-tar.backup"
cp "${bundle}/${linux_base}.tar.gz" "${linux_backup}"
printf 'unexpected cli binary\n' >"${linux_archive}/fw"
(cd "${linux_archive}" && tar -czf "${bundle}/${linux_base}.tar.gz" ./*)
refresh_checksums
assert_archive_rejects_extra_member "${bundle}/${linux_base}.tar.gz" linux-tar "${linux_backup}"

for arch in arm64 x86_64; do
  mac_base="FlistWalker-${version}-macos-${arch}"
  mac_archive="${work_dir}/mac-${arch}"
  mac_backup="${work_dir}/mac-${arch}-tar.backup"
  cp "${bundle}/${mac_base}.tar.gz" "${mac_backup}"
  printf 'unexpected cli binary\n' >"${mac_archive}/fw"
  (cd "${mac_archive}" && tar -czf "${bundle}/${mac_base}.tar.gz" ./*)
  refresh_checksums
  assert_archive_rejects_extra_member "${bundle}/${mac_base}.tar.gz" "mac-${arch}-tar" "${mac_backup}"
done

bash "${repo_dir}/scripts/validate-release-bundle.sh" "${tag}" "${bundle}"

printf 'unexpected\n' >"${bundle}/unexpected.txt"
if bash "${repo_dir}/scripts/validate-release-bundle.sh" "${tag}" "${bundle}" >/dev/null 2>&1; then
  echo "validator accepted an unexpected asset" >&2
  exit 1
fi

echo "release bundle validator regression tests passed"
