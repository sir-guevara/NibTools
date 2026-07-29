#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: package.sh <platform> <binary-path>" >&2
  exit 2
fi

platform="$1"
binary_path="$2"

if [[ ! -x "$binary_path" ]]; then
  echo "binary is missing or not executable: $binary_path" >&2
  exit 1
fi

package_name="nibpass-${platform}"
dist_dir="dist"
package_dir="${dist_dir}/${package_name}"

rm -rf "$package_dir"
mkdir -p "${package_dir}/bin" "${package_dir}/completions"

cp "$binary_path" "${package_dir}/bin/nibpass"
cp README.md "${package_dir}/README.md"
cp install.sh "${package_dir}/install.sh"
chmod 755 "${package_dir}/bin/nibpass" "${package_dir}/install.sh"

"${package_dir}/bin/nibpass" completion zsh > "${package_dir}/completions/_nibpass"
"${package_dir}/bin/nibpass" completion bash > "${package_dir}/completions/nibpass.bash"
"${package_dir}/bin/nibpass" completion fish > "${package_dir}/completions/nibpass.fish"

tar -C "$dist_dir" -czf "${dist_dir}/${package_name}.tar.gz" "$package_name"
