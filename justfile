set dotenv-load := true

fmt:
    cargo fmt --check

lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
    cargo test --workspace --all-targets --all-features

package-dry-run:
    bash npm/testing/test-publish-packages.sh

ci: fmt lint test package-dry-run
    bash npm/testing/validate.sh
    node npm/testing/test-platform-detection.js

real-e2e:
    CLAUDE_CODE_ACP_REAL_E2E=1 cargo test --test real_e2e -- --ignored --test-threads=1

doctor-live:
    cargo run -- doctor --live-docs

drift-live:
    CLAUDE_CODE_ACP_LIVE_DOCS=1 cargo test --test upstream_drift -- --ignored
