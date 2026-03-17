#!/usr/bin/env bash
set -euo pipefail

flistwalker_windows_target() {
  printf '%s\n' "${FLISTWALKER_WINDOWS_TARGET:-x86_64-pc-windows-gnu}"
}

flistwalker_find_tool() {
  local env_name="$1"
  shift
  local value="${!env_name:-}"
  if [[ -n "${value}" && -x "${value}" ]]; then
    printf '%s\n' "${value}"
    return 0
  fi
  local name
  for name in "$@"; do
    if command -v "${name}" >/dev/null 2>&1; then
      command -v "${name}"
      return 0
    fi
  done
  return 1
}

flistwalker_require_windows_gnu_tools() {
  local target
  target="$(flistwalker_windows_target)"
  if [[ "${target}" != "x86_64-pc-windows-gnu" ]]; then
    echo "Unsupported Windows target: ${target}" >&2
    return 1
  fi

  local linker cxx ar ranlib windres strip
  linker="$(flistwalker_find_tool FLISTWALKER_WINDOWS_LINKER x86_64-w64-mingw32-gcc || true)"
  cxx="$(flistwalker_find_tool FLISTWALKER_WINDOWS_CXX x86_64-w64-mingw32-g++ || true)"
  ar="$(flistwalker_find_tool FLISTWALKER_WINDOWS_AR x86_64-w64-mingw32-ar ar || true)"
  ranlib="$(flistwalker_find_tool FLISTWALKER_WINDOWS_RANLIB x86_64-w64-mingw32-ranlib ranlib || true)"
  windres="$(flistwalker_find_tool FLISTWALKER_WINDOWS_WINDRES x86_64-w64-mingw32-windres windres || true)"
  strip="$(flistwalker_find_tool FLISTWALKER_WINDOWS_STRIP x86_64-w64-mingw32-strip strip || true)"

  if [[ -z "${linker}" || -z "${cxx}" || -z "${ar}" || -z "${ranlib}" || -z "${windres}" || -z "${strip}" ]]; then
    return 1
  fi

  export FLISTWALKER_WINDOWS_LINKER="${linker}"
  export FLISTWALKER_WINDOWS_CXX="${cxx}"
  export FLISTWALKER_WINDOWS_AR="${ar}"
  export FLISTWALKER_WINDOWS_RANLIB="${ranlib}"
  export FLISTWALKER_WINDOWS_WINDRES="${windres}"
  export FLISTWALKER_WINDOWS_STRIP="${strip}"
  export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="${linker}"
  export CC_x86_64_pc_windows_gnu="${linker}"
  export CXX_x86_64_pc_windows_gnu="${cxx}"
  export AR_x86_64_pc_windows_gnu="${ar}"
  export RANLIB_x86_64_pc_windows_gnu="${ranlib}"
}

flistwalker_print_windows_gnu_help() {
  cat >&2 <<'EOF'
mingw-w64 ツールが不足しています。WSL/Linux から Windows GNU ビルドするには、
少なくとも次の実行ファイルが必要です:
  - x86_64-w64-mingw32-gcc
  - x86_64-w64-mingw32-g++
  - x86_64-w64-mingw32-ar
  - x86_64-w64-mingw32-ranlib
  - x86_64-w64-mingw32-windres
  - x86_64-w64-mingw32-strip

Ubuntu / Debian 系の例:
  sudo apt update
  sudo apt install -y gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 binutils-mingw-w64-x86-64

個別指定する場合は FLISTWALKER_WINDOWS_LINKER / _CXX / _AR / _RANLIB / _WINDRES / _STRIP を設定してください。
EOF
}
