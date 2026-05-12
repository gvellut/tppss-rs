#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SIGNING_IDENTITY="${SIGNING_IDENTITY:-My Swift Dev Cert}"
EXECUTABLE_PATH="${EXECUTABLE_PATH:-}"

product_identifier() {
    local product_name="$1"

    case "$product_name" in
        tppss)
            printf '%s\n' "com.vellut.tppss.cli.dev"
            ;;
        *)
            printf '%s\n' "com.vellut.${product_name}.dev"
            ;;
    esac
}

build_product() {
    local product_name="$1"
    local configuration="${2:-debug}"
    shift 2 || true
    local -a build_args=(build --package tppss-cli --bin "$product_name")
    local profile_dir="debug"

    case "$configuration" in
        debug) ;;
        release)
            build_args+=(--release)
            profile_dir="release"
            ;;
        *)
            echo "Unsupported configuration: $configuration" >&2
            return 1
            ;;
    esac

    if [[ $# -gt 0 ]]; then
        build_args+=("$@")
    fi

    cargo "${build_args[@]}"

    local target_dir="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"
    EXECUTABLE_PATH="$target_dir/$profile_dir/$product_name"

    if [[ ! -x "$EXECUTABLE_PATH" ]]; then
        echo "Built product not found at $EXECUTABLE_PATH" >&2
        return 1
    fi
}

sign_built_product() {
    local product_name="$1"
    local configuration="${2:-debug}"
    shift 2 || true
    local identifier=""
    local codesign_output=""

    build_product "$product_name" "$configuration" "$@"
    identifier="$(product_identifier "$product_name")"

    if ! codesign_output="$(
        codesign \
            --force \
            --sign "$SIGNING_IDENTITY" \
            --identifier "$identifier" \
            --timestamp=none \
            "$EXECUTABLE_PATH" \
            2>&1
    )"; then
        printf '%s\n' "$codesign_output" >&2
        cat >&2 <<EOF

Failed to sign "$EXECUTABLE_PATH" with identity "$SIGNING_IDENTITY".

Check available signing identities with:
  security find-identity -v -p codesigning

Or rerun with:
  SIGNING_IDENTITY="Your Certificate Name" ./scripts/sign-built-product.sh "$product_name" "$configuration"
EOF
        return 1
    fi

    codesign --verify --strict "$EXECUTABLE_PATH"
}

usage() {
    cat >&2 <<'EOF'
usage: scripts/sign-built-product.sh <product-name> [debug|release] [cargo build args...]

examples:
  scripts/sign-built-product.sh tppss debug --features tppss/gcs,tppss-cli/gcs
  scripts/sign-built-product.sh tppss release --features tppss/s3,tppss-cli/s3
EOF
}

main() {
    if [[ $# -lt 1 ]]; then
        usage
        exit 1
    fi

    local product_name="$1"
    local configuration="${2:-debug}"
    if [[ $# -ge 2 ]]; then
        shift 2
    else
        shift 1
    fi

    sign_built_product "$product_name" "$configuration" "$@"

    echo "Built and signed:"
    echo "  $EXECUTABLE_PATH"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
