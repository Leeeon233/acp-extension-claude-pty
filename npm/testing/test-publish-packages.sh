#!/usr/bin/env bash
set -euo pipefail

check_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Required command not found: $1" >&2
    exit 1
  fi
}

check_command node
check_command npm
check_command tar
check_command zip
check_command unzip

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

artifacts_dir="$tmp_dir/artifacts"
packages_dir="$tmp_dir/npm-packages"
staging_dir="$tmp_dir/staging"
version="9.8.7"

mkdir -p "$artifacts_dir" "$staging_dir"

targets=(
  "aarch64-apple-darwin:"
  "x86_64-apple-darwin:"
  "x86_64-unknown-linux-gnu:"
  "aarch64-unknown-linux-gnu:"
  "x86_64-pc-windows-msvc:.exe"
  "aarch64-pc-windows-msvc:.exe"
)

for target_spec in "${targets[@]}"; do
  target="${target_spec%%:*}"
  ext="${target_spec#*:}"
  target_dir="$staging_dir/$target"
  archive_name="acp-extension-claude-pty-${version}-${target}"
  mkdir -p "$target_dir"
  printf '#!/usr/bin/env sh\nexit 0\n' > "$target_dir/acp-extension-claude-pty${ext}"
  chmod +x "$target_dir/acp-extension-claude-pty${ext}"

  if [[ "$target" == *"windows"* ]]; then
    artifact_dir="$artifacts_dir/${archive_name}.zip"
    mkdir -p "$artifact_dir"
    (cd "$target_dir" && zip -q "$artifact_dir/${archive_name}.zip" "acp-extension-claude-pty${ext}")
  else
    artifact_dir="$artifacts_dir/${archive_name}.tar.gz"
    mkdir -p "$artifact_dir"
    tar czf "$artifact_dir/${archive_name}.tar.gz" -C "$target_dir" "acp-extension-claude-pty${ext}"
  fi
done

bash npm/publish/create-platform-packages.sh "$artifacts_dir" "$packages_dir" "$version"

expected_packages=(
  "acp-extension-claude-pty-darwin-arm64"
  "acp-extension-claude-pty-darwin-x64"
  "acp-extension-claude-pty-linux-arm64"
  "acp-extension-claude-pty-linux-x64"
  "acp-extension-claude-pty-win32-arm64"
  "acp-extension-claude-pty-win32-x64"
)

for package_name in "${expected_packages[@]}"; do
  package_dir="$packages_dir/$package_name"
  test -f "$package_dir/package.json"
  test -f "$package_dir/README.md"
  test -f "$package_dir/LICENSE"
  test -f "$package_dir/NOTICE.md"
  node - <<'NODE' "$package_dir/package.json"
const fs = require("fs");
const pkg = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
if (pkg.bin) {
  throw new Error(`${pkg.name} must not declare a bin; it conflicts with the base wrapper bin`);
}
NODE
  npm pack --dry-run --json "$package_dir" >/dev/null
done

npm pack --dry-run --json ./npm >/dev/null

base_dir="$tmp_dir/base-package"
install_dir="$tmp_dir/install-check"
cp -R npm "$base_dir"

node - <<'NODE' "$base_dir/package.json" "$packages_dir"
const fs = require("fs");
const path = require("path");
const packageJsonPath = process.argv[2];
const packagesDir = process.argv[3];
const pkg = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
for (const packageName of Object.keys(pkg.optionalDependencies)) {
  pkg.optionalDependencies[packageName] = `file:${path.join(packagesDir, packageName)}`;
}
fs.writeFileSync(packageJsonPath, `${JSON.stringify(pkg, null, 2)}\n`);
NODE

mkdir -p "$install_dir"
(
  cd "$install_dir"
  npm init -y >/dev/null
  npm install --silent "$base_dir"
  test -x node_modules/.bin/acp-extension-claude-pty
)

echo "Publish package dry-run checks passed."
