#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -eq 0 ]; then
  printf 'usage: %s command [args...]\n' "$0" >&2
  exit 2
fi

repo_root="$(git rev-parse --show-toplevel)"
cache_root="${OSL_NATIVE_DEPS_CACHE:-$repo_root/.cache/osl-native-deps}"
deb_dir="$cache_root/deb"
root_dir="$cache_root/root"
mkdir -p "$deb_dir" "$root_dir"

packages=(
  "libclang1-18=1:18.1.3-1ubuntu1"
  "libllvm18=1:18.1.3-1ubuntu1"
  "cmake=3.28.3-1build7"
  "cmake-data=3.28.3-1build7"
  "libarchive13t64=3.7.2-2ubuntu0.8"
  "libjsoncpp25=1.9.5-6build1"
  "librhash0=1.4.3-3build1"
)

for spec in "${packages[@]}"; do
  name="${spec%%=*}"
  version="${spec#*=}"
  marker="$root_dir/.${name}_${version//[:\/]/_}.installed"
  if [ ! -e "$marker" ]; then
    rm -f "$deb_dir/${name}_"*.deb
    (
      cd "$deb_dir"
      apt-get download "$spec"
    )
    deb="$(find "$deb_dir" -maxdepth 1 -type f -name "${name}_*.deb" | sort | tail -n 1)"
    test -n "$deb"
    dpkg-deb -x "$deb" "$root_dir"
    : > "$marker"
  fi
done

clang_dir="$root_dir/usr/lib/llvm-18/lib"
test -r "$clang_dir/libclang-18.so.1"
export LIBCLANG_PATH="$clang_dir"
export LD_LIBRARY_PATH="$root_dir/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export PATH="$root_dir/usr/bin:$PATH"
cc_include="$("${CC:-cc}" -print-file-name=include)"
if [ -d "$cc_include" ]; then
  export BINDGEN_EXTRA_CLANG_ARGS="-isystem $cc_include${BINDGEN_EXTRA_CLANG_ARGS:+ $BINDGEN_EXTRA_CLANG_ARGS}"
fi

printf 'OSL_NATIVE_COVER_BUILD_LIB=%s\n' "${packages[0]}"
exec "$@"
